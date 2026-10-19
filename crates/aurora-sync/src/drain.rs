//! DK-40b: 离线队列 → iroh 传输的补发衔接层。
//!
//! 职责：网络恢复后把 `OfflineQueue` 中积压的待同步文档补发给对端。
//! 设计要点：
//! - **resolver 解耦**：drain 不感知文档存储细节，由调用方注入
//!   `doc_id → LoroDoc` 解析闭包（DI 惯例，装配层在 aurora-bootstrap）。
//! - **文档级状态同步**：iroh 面是版本向量交换的增量导入，同一文档多次
//!   积压 item 只需一次 `sync_with_peer`——队列按 `doc_id` 去重。
//! - **失败回队**：`SyncReport.success == false` 或 resolver 未命中的 item
//!   走 `OfflineQueue::requeue`（attempts 已递增），不影响幂等索引。
//! - **成功 ack**：清理幂等索引，队列收敛为空。

use crate::iroh_transport::{IrohTransport, SyncReport};
use crate::offline_queue::{OfflineQueue, QueueItem};
use iroh::EndpointAddr;
use loro::LoroDoc;
use std::collections::BTreeSet;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// 单次 drain 的结果汇总。
#[derive(Debug, Default, Clone)]
pub struct DrainReport {
    /// 成功补发的文档数（去重后）。
    pub synced_docs: usize,
    /// 失败回队的文档 ID（保留在队列中，下次恢复重试）。
    pub failed_docs: Vec<String>,
    /// ack 掉的队列项数（≤ 出队项数，同文档多项只计一次同步）。
    pub acked_items: usize,
}

/// 将队列中最多 `batch` 条积压项补发给 `peer_addr`。
///
/// * `doc_resolver`：`doc_id → LoroDoc`；返回 `None` 视为本次失败（回队）。
/// * 出队批次内按 `doc_id` 去重后逐文档同步；成功则 ack 该文档全部积压项。
pub async fn drain_to_peer(
    queue: &OfflineQueue,
    transport: &IrohTransport,
    peer_addr: &EndpointAddr,
    doc_resolver: &dyn Fn(&str) -> Option<Arc<LoroDoc>>,
    batch: usize,
) -> DrainReport {
    drain_inner(queue, transport, peer_addr, &|id| doc_resolver(id), batch).await
}

/// DK-44: Send+Sync 约束变体——tauri command 的 future 必须 Send，
/// `&dyn Fn`（?Sync）无法跨 await。逻辑与 [`drain_to_peer`] 一致
/// （复制实现，仅 doc_resolver 约束收紧为 Send+Sync；两份须同步维护）。
pub async fn drain_to_peer_sync(
    queue: &OfflineQueue,
    transport: &IrohTransport,
    peer_addr: &EndpointAddr,
    doc_resolver: &(dyn Fn(&str) -> Option<Arc<LoroDoc>> + Send + Sync),
    batch: usize,
) -> DrainReport {
    let items: Vec<QueueItem> = queue.dequeue_batch(batch);
    if items.is_empty() {
        return DrainReport::default();
    }
    debug!("drain_sync: 出队 {} 项", items.len());

    let mut report = DrainReport::default();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut ack_keys: Vec<String> = Vec::with_capacity(items.len());
    let mut failed: BTreeSet<String> = BTreeSet::new();

    for item in &items {
        if failed.contains(&item.doc_id) {
            continue;
        }
        if seen.contains(&item.doc_id) {
            ack_keys.push(item.idempotency_key.clone());
            report.acked_items += 1;
            continue;
        }
        seen.insert(item.doc_id.clone());
        let Some(doc) = doc_resolver(&item.doc_id) else {
            warn!("drain_sync: doc 解析失败 doc_id={}", item.doc_id);
            failed.insert(item.doc_id.clone());
            report.failed_docs.push(item.doc_id.clone());
            continue;
        };
        let sync: SyncReport = match transport.sync_with_peer(peer_addr.clone(), &doc).await {
            Ok(r) => r,
            Err(e) => {
                warn!("drain_sync: doc={} 传输错误 {}", item.doc_id, e);
                failed.insert(item.doc_id.clone());
                report.failed_docs.push(item.doc_id.clone());
                continue;
            }
        };
        if sync.success {
            info!(
                "drain_sync: doc={} 补发成功 sent={}B recv={}B",
                item.doc_id, sync.sent_bytes, sync.received_bytes
            );
            ack_keys.push(item.idempotency_key.clone());
            report.acked_items += 1;
            report.synced_docs += 1;
        } else {
            warn!("drain_sync: doc={} 补发失败 {:?}", item.doc_id, sync.error);
            failed.insert(item.doc_id.clone());
            report.failed_docs.push(item.doc_id.clone());
        }
    }

    for key in &ack_keys {
        queue.ack(key).expect("ack 清理幂等索引不应失败");
    }
    for item in items {
        if failed.contains(&item.doc_id) {
            queue.requeue(item);
        }
    }
    report
}

/// drain 共享实现（R: Fn + Send + Sync——两条入口约束各自满足）。
async fn drain_inner<R>(
    queue: &OfflineQueue,
    transport: &IrohTransport,
    peer_addr: &EndpointAddr,
    doc_resolver: &R,
    batch: usize,
) -> DrainReport
where
    R: Fn(&str) -> Option<Arc<LoroDoc>>,
{
    let items: Vec<QueueItem> = queue.dequeue_batch(batch);
    if items.is_empty() {
        return DrainReport::default();
    }
    debug!("drain: 出队 {} 项", items.len());

    let mut report = DrainReport::default();
    // 按 doc_id 去重保序（BTreeSet 保证确定性遍历顺序）。
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut ack_keys: Vec<String> = Vec::with_capacity(items.len());
    let mut failed: BTreeSet<String> = BTreeSet::new();

    for item in &items {
        if failed.contains(&item.doc_id) {
            // 同文档已有失败：本项不 ack，随文档一起留在队列语义中。
            continue;
        }
        if seen.contains(&item.doc_id) {
            ack_keys.push(item.idempotency_key.clone());
            report.acked_items += 1;
            continue;
        }
        seen.insert(item.doc_id.clone());
        let Some(doc) = doc_resolver(&item.doc_id) else {
            warn!("drain: doc 解析失败 doc_id={}", item.doc_id);
            failed.insert(item.doc_id.clone());
            report.failed_docs.push(item.doc_id.clone());
            continue;
        };
        let sync: SyncReport = match transport.sync_with_peer(peer_addr.clone(), &doc).await {
            Ok(r) => r,
            Err(e) => {
                warn!("drain: doc={} 传输错误 {}", item.doc_id, e);
                failed.insert(item.doc_id.clone());
                report.failed_docs.push(item.doc_id.clone());
                continue;
            }
        };
        if sync.success {
            info!(
                "drain: doc={} 补发成功 sent={}B recv={}B",
                item.doc_id, sync.sent_bytes, sync.received_bytes
            );
            ack_keys.push(item.idempotency_key.clone());
            report.acked_items += 1;
            report.synced_docs += 1;
        } else {
            warn!("drain: doc={} 补发失败 {:?}", item.doc_id, sync.error);
            failed.insert(item.doc_id.clone());
            report.failed_docs.push(item.doc_id.clone());
        }
    }

    // 成功项 ack；失败项整体回队（含同文档的其余积压项）。
    for key in &ack_keys {
        queue.ack(key).expect("ack 清理幂等索引不应失败");
    }
    for item in items {
        if failed.contains(&item.doc_id) {
            queue.requeue(item);
        }
    }
    report
}

// ============================================================================
// DK-T4: 单 doc 选择性补发（2026-10-19 台账 T4 消化——回执建议序首位）
// ============================================================================

/// 选出某 doc 的全部积压项（**peek 语义**——不出队不 ack，失败路径队列原样）。
pub fn select_queue_items(queue: &OfflineQueue, doc_id: &str) -> Vec<QueueItem> {
    queue
        .items()
        .into_iter()
        .filter(|item| item.doc_id == doc_id)
        .collect()
}

/// ack 指定积压项（按幂等键清理——成功路径收尾；ack 幂等，重复调用安全）。
pub fn ack_queue_items(queue: &OfflineQueue, items: &[QueueItem]) {
    for item in items {
        queue
            .ack(&item.idempotency_key)
            .expect("ack 清理幂等索引不应失败");
    }
}

/// 单 doc 选择性补发（T4）：解析与门禁由宿主完成，本函数只做
/// 「peek 目标 doc 项 → `sync_with_peer` → 成功才 ack」编排。
///
/// **失败路径队列原样**（items 不动——与全量 drain 的 dequeue/requeue 不同，
/// peek+ack 语义下无需回队），显式错误返回由宿主提示。
///
/// # Errors
/// 队列中无该 doc 项 / 传输错误 / `SyncReport.success == false`。
pub async fn drain_one_doc_sync(
    queue: &OfflineQueue,
    transport: &IrohTransport,
    peer_addr: &EndpointAddr,
    doc_id: &str,
    doc: Arc<LoroDoc>,
) -> Result<DrainReport, String> {
    let items = select_queue_items(queue, doc_id);
    if items.is_empty() {
        return Err(format!("队列中无 doc_id={doc_id} 的待补发项"));
    }
    let sync: SyncReport = transport
        .sync_with_peer(peer_addr.clone(), &doc)
        .await
        .map_err(|e| format!("doc={doc_id} 传输错误: {e}"))?;
    if !sync.success {
        return Err(format!("doc={doc_id} 补发失败: {:?}", sync.error));
    }
    let mut report = DrainReport {
        synced_docs: 1,
        ..Default::default()
    };
    ack_queue_items(queue, &items);
    report.acked_items = items.len();
    info!("drain_one_doc: doc={} 成功 ack {} 项", doc_id, items.len());
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::offline_queue::Priority;

    /// 失败回队：resolver 未命中 → item 回队（attempts 递增、幂等键保留）。
    #[test]
    fn test_requeue_keeps_item_and_index() {
        let q = OfflineQueue::new();
        let item = QueueItem::new("doc-x", vec![1], Priority::High);
        let key = item.idempotency_key.clone();
        q.enqueue(item).unwrap();

        let popped = q.dequeue().unwrap();
        assert_eq!(popped.attempts, 1);
        q.requeue(popped);

        assert_eq!(q.len(), 1, "失败项必须留在队列中");
        assert!(q.contains_key(&key), "失败项幂等键不能被 ack 清理");
        // 回队项仍可再次出队（attempts 继续递增）。
        let again = q.dequeue().unwrap();
        assert_eq!(again.attempts, 2);
    }

    /// DK-T4: select/ack 语义——peek 只筛目标 doc 不动队列；ack 后幂等键
    /// 清理、他 doc 项不受影响。
    #[test]
    fn test_t4_select_and_ack_semantics() {
        let q = OfflineQueue::new();
        let k1 = QueueItem::new("doc-a", vec![1], Priority::High);
        let k2 = QueueItem::new("doc-a", vec![2], Priority::Low);
        let kb = QueueItem::new("doc-b", vec![3], Priority::Normal);
        let (key1, key2, keyb) = (
            k1.idempotency_key.clone(),
            k2.idempotency_key.clone(),
            kb.idempotency_key.clone(),
        );
        for item in [k1, k2, kb] {
            q.enqueue(item).unwrap();
        }

        // peek：只选 doc-a 两项（不出队——len 不变）
        let picked = select_queue_items(&q, "doc-a");
        assert_eq!(picked.len(), 2, "doc-a 两项须全被选中");
        assert_eq!(q.len(), 3, "peek 不得出队");
        assert!(select_queue_items(&q, "doc-absent").is_empty());

        // 空项错误路径语义（drain_one_doc 前置）由宿主判——此处验 ack 面：
        ack_queue_items(&q, &picked);
        assert!(q.contains_key(&keyb), "他 doc 项不受影响");
        assert!(!q.contains_key(&key1), "doc-a 项一幂等键已清理");
        assert!(!q.contains_key(&key2), "doc-a 项二幂等键已清理");
        // 队列本体仍含 doc-b（ack 只清幂等索引——dequeue 面由既有语义管）
        assert_eq!(select_queue_items(&q, "doc-a").len(), 0, "ack 后不再被选中");
    }
}
