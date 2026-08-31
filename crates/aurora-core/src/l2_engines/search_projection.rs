//! 搜索索引投影 — V20 §3.2/§4.5（Phase 1 核心基石）
//!
//! 「单一事实源 + 投影读模型」的搜索侧落地：
//! Loro OpLog / EventBus 是事实源；Tantivy 全文索引是可重放重建的投影。
//!
//! # 事件映射
//!
//! | AppEvent              | 索引动作            |
//! |-----------------------|---------------------|
//! | `NoteCreated`         | `index_note`        |
//! | `NoteMetadataChanged` | 重索引（标题变更）  |
//! | `NoteDeleted`         | `remove_index`      |
//! | 其他                  | no-op（幂等忽略）   |
//!
//! # 崩溃恢复
//!
//! - **增量**：水位线持久化于 KVStore（`projection.watermark.search`），
//!   启动时 [`LayeredEventBus::catch_up`] 重放 `seq > watermark` 的事件
//! - **全量**：`verify` 失败或手动触发时 `rebuild`（从数据源回调拉取
//!   全部笔记重建索引）
//!
//! # 跨通道顺序（§32.2）
//!
//! 消费循环配合 [`LayeredEventBus::low_ready`]：本投影消费的事件若有
//! 前序 Medium 未 ack，等待 watermark 追赶后再 apply。

use std::sync::Arc;

use async_trait::async_trait;
use tracing::{debug, info, warn};

use crate::event_bus::layered::AppEvent;
use crate::event_bus::projection::{Projection, ProjectionHealth};
use crate::l1_infrastructure::storage_engine::MemoryKVStore;
use crate::traits::kv_store::KVStore;
use crate::traits::search_backend::{IndexEntry, NoteMetadata, SearchBackend, SearchOptions};

/// 搜索索引投影。
///
/// 事件 → 全文索引的单向投影；索引损坏/落后时通过
/// [`LayeredEventBus::catch_up`] 增量追赶或全量重建恢复，
/// 用户数据（Loro OpLog）永不因索引故障受损。
pub struct SearchIndexProjection {
    search: Arc<dyn SearchBackend>,
    kv: Arc<dyn KVStore>,
    /// 全量数据源（rebuild 时拉取全部笔记），由装配层注入。
    source: Box<dyn Fn() -> Vec<IndexEntry> + Send + Sync>,
}

const WM_KEY: &str = "projection.watermark.search";

impl SearchIndexProjection {
    /// 创建投影。`source` 供全量重建（如 KV 中全部笔记条目）。
    pub fn new(
        search: Arc<dyn SearchBackend>,
        kv: Arc<dyn KVStore>,
        source: Box<dyn Fn() -> Vec<IndexEntry> + Send + Sync>,
    ) -> Self {
        Self { search, kv, source }
    }

    /// 重索引单篇（元数据变更场景 — 标题/标签变了但事件不带全文）。
    async fn reindex(&self, note_id: &str) -> Result<(), crate::Error> {
        // 事件不携带全文 → 从数据源拉该笔记最新状态（拉模式投影）
        let entry = (self.source)().into_iter().find(|e| e.note_id == note_id);
        match entry {
            Some(e) => {
                self.search
                    .index_note(&e.note_id, &e.content, &e.metadata)
                    .await
            }
            None => {
                // 数据源已无此笔记（如先删除后改名）→ 清索引兜底
                debug!(note_id, "reindex: source missing; removing stale index");
                self.search.remove_index(note_id).await
            }
        }
    }
}

#[async_trait]
impl Projection for SearchIndexProjection {
    fn name(&self) -> &'static str {
        "search-index"
    }

    async fn watermark(&self) -> Result<u64, crate::Error> {
        match self.kv.get(WM_KEY).await? {
            Some(bytes) => {
                if bytes.len() == 8 {
                    let mut b = [0u8; 8];
                    b.copy_from_slice(&bytes);
                    Ok(u64::from_le_bytes(b))
                } else {
                    Ok(0)
                }
            }
            None => Ok(0),
        }
    }

    async fn set_watermark(&self, seq: u64) -> Result<(), crate::Error> {
        self.kv.set(WM_KEY, &seq.to_le_bytes()).await
    }

    /// 幂等应用：同一事件重复投递结果一致（index_note 存在则更新）。
    async fn apply(&self, event: &AppEvent) -> Result<(), crate::Error> {
        match event {
            AppEvent::NoteCreated {
                note_id,
                title,
                content,
            } => {
                self.search
                    .index_note(note_id, content, &NoteMetadata { title: title.clone(), ..Default::default() })
                    .await?;
                debug!(note_id, "search projection: indexed");
            }
            AppEvent::NoteMetadataChanged { note_id, .. } => {
                self.reindex(note_id).await?;
            }
            AppEvent::NoteDeleted { note_id } => {
                self.search.remove_index(note_id).await?;
                debug!(note_id, "search projection: removed");
            }
            _ => {} // 幂等忽略无关事件
        }
        Ok(())
    }

    /// 轻量健康校验：水位线键可读即视为结构完好；
    /// 深度校验（索引 vs 数据源 diff）由每日全量补偿任务执行。
    async fn verify(&self) -> Result<ProjectionHealth, crate::Error> {
        Ok(match self.watermark().await {
            Ok(_) => ProjectionHealth::Ok,
            Err(e) => {
                warn!(error = %e, "search projection watermark unreadable");
                ProjectionHealth::Corrupted
            }
        })
    }

    /// 全量重建：清空并从数据源重放全部笔记。
    async fn rebuild(&self) -> Result<(), crate::Error> {
        info!("search projection: full rebuild");
        let all = (self.source)();
        self.search.rebuild_index(&all).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_bus::layered::{EventChannel, InMemoryEventQueue, LayeredEventBus, LinkAction, NoteChanges};
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// 内存 SearchBackend（测试用）。
    struct InMemSearch {
        docs: std::sync::RwLock<std::collections::BTreeMap<String, String>>,
        rebuilds: AtomicUsize,
    }

    #[async_trait]
    impl SearchBackend for InMemSearch {
        async fn search(&self, query: &str, _opts: &SearchOptions) -> Result<crate::traits::search_backend::SearchResult, crate::Error> {
            let docs = self.docs.read().unwrap();
            let hits = docs
                .iter()
                .filter(|(id, title)| id.contains(query) || title.contains(query))
                .map(|(id, title)| crate::traits::search_backend::SearchHit {
                    note_id: id.clone(),
                    title: title.clone(),
                    score: 1.0,
                    snippet: title.clone(),
                })
                .collect();
            Ok(crate::traits::search_backend::SearchResult { hits, total: docs.len(), took_ms: 0 })
        }
        async fn index_note(&self, note_id: &str, _content: &str, metadata: &NoteMetadata) -> Result<(), crate::Error> {
            self.docs.write().unwrap().insert(note_id.into(), metadata.title.clone());
            Ok(())
        }
        async fn batch_index(&self, notes: &[IndexEntry]) -> Result<(), crate::Error> {
            for n in notes {
                self.index_note(&n.note_id, &n.content, &n.metadata).await?;
            }
            Ok(())
        }
        async fn remove_index(&self, note_id: &str) -> Result<(), crate::Error> {
            self.docs.write().unwrap().remove(note_id);
            Ok(())
        }
        async fn rebuild_index(&self, all_notes: &[IndexEntry]) -> Result<(), crate::Error> {
            self.rebuilds.fetch_add(1, Ordering::SeqCst);
            let mut docs = self.docs.write().unwrap();
            docs.clear();
            for n in all_notes {
                docs.insert(n.note_id.clone(), n.metadata.title.clone());
            }
            Ok(())
        }
        fn tokenize(&self, _text: &str) -> Vec<String> {
            Vec::new()
        }
    }

    fn entry(id: &str, title: &str) -> IndexEntry {
        IndexEntry {
            note_id: id.into(),
            content: String::new(),
            metadata: NoteMetadata { title: title.into(), ..Default::default() },
        }
    }

    fn make_projection(
        search: Arc<InMemSearch>,
        kv: Arc<MemoryKVStore>,
        source: Box<dyn Fn() -> Vec<IndexEntry> + Send + Sync>,
    ) -> SearchIndexProjection {
        SearchIndexProjection::new(search, kv, source)
    }

    #[tokio::test]
    async fn note_created_and_deleted_project_to_index() {
        let bus = LayeredEventBus::new(None);
        let search = Arc::new(InMemSearch { docs: Default::default(), rebuilds: AtomicUsize::new(0) });
        let kv = Arc::new(MemoryKVStore::default());
        let proj = make_projection(search.clone(), kv, Box::new(Vec::new));

        proj.apply(&AppEvent::NoteCreated {
            note_id: "n1".into(),
            title: "Hello".into(),
            content: "world".into(),
        })
        .await
        .unwrap();
        let r = search.search("Hello", &SearchOptions::default()).await.unwrap();
        assert_eq!(r.hits.len(), 1);

        proj.apply(&AppEvent::NoteDeleted { note_id: "n1".into() }).await.unwrap();
        let r = search.search("Hello", &SearchOptions::default()).await.unwrap();
        assert_eq!(r.hits.len(), 0);
    }

    #[tokio::test]
    async fn unrelated_events_are_idempotent_noops() {
        let search = Arc::new(InMemSearch { docs: Default::default(), rebuilds: AtomicUsize::new(0) });
        let kv = Arc::new(MemoryKVStore::default());
        let proj = make_projection(search.clone(), kv, Box::new(Vec::new));

        for ev in [
            AppEvent::CursorMoved { note_id: "n".into(), user_id: "u".into(), pos: 1 },
            AppEvent::BidiLinkChanged {
                source_note_id: "a".into(),
                target_note_id: "b".into(),
                action: LinkAction::Created,
            },
            AppEvent::SnapshotRequested { note_id: "n".into(), label: None },
        ] {
            proj.apply(&ev).await.unwrap();
        }
        let r = search.search("", &SearchOptions::default()).await.unwrap();
        assert_eq!(r.hits.len(), 0);
    }

    #[tokio::test]
    async fn metadata_changed_reindexes_from_source() {
        let search = Arc::new(InMemSearch { docs: Default::default(), rebuilds: AtomicUsize::new(0) });
        let kv = Arc::new(MemoryKVStore::default());
        // 数据源: n1 标题已是「新标题」
        let proj = make_projection(search.clone(), kv, Box::new(|| vec![entry("n1", "新标题")]));

        // 事件不带全文（拉模式）
        proj.apply(&AppEvent::NoteMetadataChanged {
            note_id: "n1".into(),
            changes: NoteChanges { title: Some("新标题".into()), ..Default::default() },
        })
        .await
        .unwrap();
        let r = search.search("新标题", &SearchOptions::default()).await.unwrap();
        assert_eq!(r.hits.len(), 1);
    }

    /// Phase 1 退出条件验证（V20 §6.1）: 杀进程后索引自动补齐。
    /// 模拟: 第一次运行消费 3 条事件后「崩溃」→ 新总线 + 同 store +
    /// 同投影水位线 → catch_up 只补增量。
    #[tokio::test]
    async fn crash_recovery_catches_up_incrementally() {
        let store = InMemoryEventQueue::new();
        let bus = LayeredEventBus::new(Some(std::sync::Arc::new(store.clone()) as std::sync::Arc<dyn crate::event_bus::layered::EventQueueStore>));
        let search = Arc::new(InMemSearch { docs: Default::default(), rebuilds: AtomicUsize::new(0) });
        let kv = Arc::new(MemoryKVStore::default());
        let proj = make_projection(search.clone(), kv.clone(), Box::new(Vec::new));

        for i in 1..=3 {
            bus.publish(AppEvent::NoteCreated {
                note_id: format!("n{i}"),
                title: format!("t{i}"),
                content: String::new(),
            });
        }
        assert_eq!(bus.catch_up(&proj).await.unwrap(), 3);

        // 「重启」: 新总线 + 同 store + 同 KV（水位线=3 已持久化）
        let bus2 = LayeredEventBus::new(Some(std::sync::Arc::new(store) as std::sync::Arc<dyn crate::event_bus::layered::EventQueueStore>));
        // 重启语义: 先恢复全局 seq（否则新事件 seq 撞历史）
        bus2.restore_seq().unwrap();
        bus2.publish(AppEvent::NoteCreated {
            note_id: "n4".into(),
            title: "t4".into(),
            content: String::new(),
        });
        let n = bus2.catch_up(&proj).await.unwrap();
        assert_eq!(n, 1, "仅追赶 seq>3 的新事件（增量）");
        let r = search.search("t", &SearchOptions::default()).await.unwrap();
        assert_eq!(r.hits.len(), 4, "索引完整补齐 4 篇");
    }

    #[tokio::test]
    async fn rebuild_restores_full_index_from_source() {
        let search = Arc::new(InMemSearch { docs: Default::default(), rebuilds: AtomicUsize::new(0) });
        let kv = Arc::new(MemoryKVStore::default());
        let all = vec![entry("n1", "A"), entry("n2", "B")];
        let proj = make_projection(search.clone(), kv, Box::new(move || all.clone()));

        proj.rebuild().await.unwrap();
        assert_eq!(search.rebuilds.load(Ordering::SeqCst), 1);
        let r = search.search("A", &SearchOptions::default()).await.unwrap();
        assert_eq!(r.hits.len(), 1);
    }

    /// §32.2 顺序约束: Medium 未 ack 时 low_ready 返回 false。
    #[test]
    fn low_ready_gating_respected_by_design() {
        let bus = LayeredEventBus::new(None);
        bus.publish(AppEvent::BidiLinkChanged {
            source_note_id: "a".into(),
            target_note_id: "b".into(),
            action: LinkAction::Created,
        });
        bus.publish(AppEvent::NoteCreated {
            note_id: "n1".into(),
            title: "t".into(),
            content: String::new(),
        });
        let mut rx = bus.take_low_receiver().unwrap();
        let ev = rx.try_recv().unwrap();
        assert_eq!(ev.event.channel(), EventChannel::Low);
        assert!(!bus.low_ready(&ev), "Medium 未 ack 前不得放行");
        bus.ack_medium(ev.pre_medium_seq.max(1));
        assert!(bus.low_ready(&ev));
    }
}
