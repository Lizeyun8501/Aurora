//! B-04: update log/blocks 派生域（write_path 拆分——纯搬移;loro-crdt
//! feature 双版本函数原样保留）。
use super::*;

/// DK-40a: 追加一条增量 update（v1 编码，38b 传输的增量单元）。
/// DK-41: pub 化——iroh sync 接收侧 sink 直写 KV updatelog（38b 桥接原语）。
#[cfg(feature = "loro-crdt")]
pub async fn append_update_log(
    kv: &dyn KVStore,
    note_id: &str,
    update: &[u8],
) -> Result<(), Error> {
    let seq = count_update_log(kv, note_id).await;
    kv.set(&update_log_key(note_id, seq), update).await
}

/// DK-41（修正 1，alpha-DK38-裁决复核.md）：接收侧原语——落对端增量并
/// **同步推进水位**。sink/宿主落 log 后若不推水位，本地下次 persist 将回落
/// 旧快照 vv 重导出对端增量 → 膨胀复发；此原语把「append+水位」绑定为单步，
/// 接收侧免知水位细节。`vv_bytes` = 应用该 update 后的 doc `oplog_vv` encode
/// （含对端 op 计数，iroh_transport 两 import 点现成可得）。
#[cfg(feature = "loro-crdt")]
pub async fn receive_update_log(
    kv: &dyn KVStore,
    note_id: &str,
    update: &[u8],
    vv_bytes: &[u8],
) -> Result<(), Error> {
    append_update_log(kv, note_id, update).await?;
    kv.set(&update_vv_key(note_id), vv_bytes).await
}

/// DK-40a: 读全部增量（seq 升序）——打开链重放用。
/// DK-41: pub 化（打开链重放/桥接测试用）。
#[cfg(feature = "loro-crdt")]
pub async fn read_update_log(kv: &dyn KVStore, note_id: &str) -> Result<Vec<Vec<u8>>, Error> {
    let pairs = kv.scan_prefix(&format!("updatelog:{note_id}:")).await?;
    let mut out: Vec<(usize, Vec<u8>)> = pairs
        .into_iter()
        .filter_map(|(k, v)| {
            let seq: usize = k.rsplit(':').next()?.parse().ok()?;
            Some((seq, v))
        })
        .collect();
    out.sort_by_key(|(seq, _)| *seq);
    Ok(out.into_iter().map(|(_, v)| v).collect())
}

/// DK-40a: log 条数（compaction 判定）。
/// DK-41: pub 化（compaction 判定/桥接测试用）。
#[cfg(feature = "loro-crdt")]
pub async fn count_update_log(kv: &dyn KVStore, note_id: &str) -> usize {
    kv.scan_prefix(&format!("updatelog:{note_id}:"))
        .await
        .map(|p| p.len())
        .unwrap_or(0)
}

/// 启动时 blocks 派生全量重建（DK-01 权威源三阶段）。
///
/// blocks 为派生索引：缺失/损坏时从 notes.content 权威重建。
/// 幂等（sync_note_blocks 软删旧块重派生）；单条派生失败仅告警不阻断
/// （重建整体继续，失败条目留待下次启动补偿）。
pub async fn rebuild_blocks_derivation(ctx: &WriteContext) -> Result<usize, Error> {
    let Some(blocks) = ctx.blocks.as_ref() else {
        return Ok(0); // 无 blocks 后端（内存降级模式）— 无需重建
    };
    let core = ctx.core.as_ref();
    let pairs = core.kv_store.scan_prefix("note:").await?;
    let mut rebuilt = 0usize;
    for (key, bytes) in &pairs {
        let Some(note_id) = key.strip_prefix("note:") else {
            continue;
        };
        // seal 端（桌面）解封读权威内容；明文端（移动）直读
        let plain = match ctx.seal.as_ref() {
            Some(seal) => match (seal.unseal)(bytes) {
                Ok(b) => b,
                Err(e) => {
                    tracing::warn!(note_id = %note_id, error = %e, "rebuild unseal failed; skip");
                    continue;
                }
            },
            None => bytes.clone(),
        };
        let Ok(record) = serde_json::from_slice::<NoteRecord>(&plain) else {
            tracing::warn!(note_id = %note_id, "rebuild deserialize failed; skip");
            continue;
        };
        if let Err(e) = blocks.sync_note_blocks(note_id, None, &record.content) {
            tracing::warn!(note_id = %note_id, error = %e, "rebuild derive failed; skip");
            continue;
        }
        rebuilt += 1;
    }
    tracing::info!(rebuilt, total = pairs.len(), "blocks derivation rebuilt");
    Ok(rebuilt)
}
