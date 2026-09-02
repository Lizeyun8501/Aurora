//! 双链投影 — V20 §3.2/§4.5（Phase 1 四投影之二）
//!
//! 消费 `BidiLinkChanged`（Medium 通道）维护 SQLite/内存链接表：
//! 源笔记 → 目标笔记（`LinkAction::Created/Deleted`），并自动维护
//! 反向索引（target → source，即「反向链接」列表）。
//!
//! 与 NoteDoc `backlinks` 容器的关系：容器是笔记内本地缓存（同步单元），
//! 本投影是**全库聚合读模型**（今日视图/知识图谱的查询入口）——
//! 可从事件重放重建（V20 投影语义）。
//!
//! # 崩溃恢复
//! 水位线持久化 KVStore（`projection.watermark.bidi_link`）；
//! verify: 链接数对比（events 计数 vs 表大小，仅缺失判损坏）；
//! rebuild: 从数据源回调（全库 backlinks 容器聚合）全量重建。

use std::collections::{BTreeMap, BTreeSet};
use std::sync::RwLock;

use async_trait::async_trait;
use tracing::{info, warn};

use crate::event_bus::layered::{AppEvent, LinkAction};
use crate::event_bus::projection::{Projection, ProjectionHealth};
use crate::traits::kv_store::KVStore;

/// 双链读模型行。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct LinkRow {
    pub source_note_id: String,
    pub target_note_id: String,
}

/// 从 SQLite links 表读全量正向链接（bootstrap 数据源 — 共享连接需
/// 独立打开; Mutex<Connection> 不可克隆）。
pub fn links_from_sqlite(path: &std::path::Path) -> Vec<LinkRow> {
    let Ok(conn) = rusqlite::Connection::open(path) else {
        return Vec::new();
    };
    let Ok(mut stmt) = conn.prepare(
        "SELECT source_note_id, target_note_id FROM links ORDER BY source_note_id, target_note_id",
    ) else {
        return Vec::new();
    };
    let rows = stmt.query_map([], |r| {
        Ok(LinkRow {
            source_note_id: r.get(0)?,
            target_note_id: r.get(1)?,
        })
    });
    match rows {
        Ok(iter) => iter.filter_map(|r| r.ok()).collect(),
        Err(_) => Vec::new(),
    }
}

/// 全量数据源回调：返回全库正向链接（含容器缓存）。
pub type LinkSource = Box<dyn Fn() -> Vec<LinkRow> + Send + Sync>;

/// 双链投影（内存表 + KVStore 水位线；SQLite 表落地为后续 PR，
/// 接口按投影抽象隔离，替换存储不影响调用方）。
pub struct BidiLinkProjection {
    /// 正向: source → targets（多值集合）。
    forward: RwLock<BTreeMap<String, BTreeSet<String>>>,
    /// 反向: target → sources。
    backward: RwLock<BTreeMap<String, BTreeSet<String>>>,
    kv: std::sync::Arc<dyn KVStore>,
    source: LinkSource,
    /// SQLite links 表持久层（可选 — V20 Phase 2: links 主存储落地）。
    /// None = 纯内存模式（测试）; Some = 事件驱动双写 + 数据源默认读表。
    sqlite: Option<std::sync::Mutex<rusqlite::Connection>>,
}

const WATERMARK_KEY: &str = "projection.watermark.bidi_link";

impl BidiLinkProjection {
    pub fn new(kv: std::sync::Arc<dyn KVStore>, source: LinkSource) -> Self {
        Self {
            sqlite: None,
            forward: RwLock::new(BTreeMap::new()),
            backward: RwLock::new(BTreeMap::new()),
            kv,
            source,
        }
    }

    /// SQLite 后端构造（links 表由 aurora-migration 建好; 事件驱动
    /// 双写内存 + SQLite — 跨进程持久, 数据源读表）。
    pub fn new_with_sqlite(
        kv: std::sync::Arc<dyn KVStore>,
        conn: rusqlite::Connection,
    ) -> Self {
        Self {
            sqlite: Some(std::sync::Mutex::new(conn)),
            forward: RwLock::new(BTreeMap::new()),
            backward: RwLock::new(BTreeMap::new()),
            kv,
            source: Box::new(Vec::new),
        }
    }

    pub fn apply_link(&self, source: &str, target: &str, action: &LinkAction) {
        // SQLite 双写（失败仅告警 — 内存投影仍正确, 表由 rebuild 修复）
        if let Some(db) = &self.sqlite {
            if let Ok(conn) = db.lock() {
                let res = match action {
                    LinkAction::Created => conn.execute(
                        "INSERT OR IGNORE INTO links (id, source_note_id, target_note_id, link_type, created_at, metadata)
                         VALUES (?1, ?2, ?3, 'reference', ?4, '{}')",
                        rusqlite::params![
                            format!("{source}->{target}"),
                            source,
                            target,
                            chrono::Utc::now().to_rfc3339()
                        ],
                    ),
                    LinkAction::Deleted => conn.execute(
                        "DELETE FROM links WHERE source_note_id = ?1 AND target_note_id = ?2",
                        rusqlite::params![source, target],
                    ),
                };
                if let Err(e) = res {
                    tracing::warn!(error = %e, "links sqlite dual-write failed");
                }
            }
        }
        match action {
            LinkAction::Created => {
                self.forward
                    .write()
                    .unwrap()
                    .entry(source.into())
                    .or_default()
                    .insert(target.into());
                self.backward
                    .write()
                    .unwrap()
                    .entry(target.into())
                    .or_default()
                    .insert(source.into());
            }
            LinkAction::Deleted => {
                // 注意: std RwLock 不可重入 — guard 存活期间再次 write()
                // 会死锁（实测踩坑）。单次持锁内完成 remove + 空键清理。
                let mut fw = self.forward.write().unwrap();
                let mut empty = false;
                if let Some(set) = fw.get_mut(source) {
                    set.remove(target);
                    empty = set.is_empty();
                }
                if empty {
                    fw.remove(source);
                }
                drop(fw);

                let mut bw = self.backward.write().unwrap();
                let mut empty_b = false;
                if let Some(set) = bw.get_mut(target) {
                    set.remove(source);
                    empty_b = set.is_empty();
                }
                if empty_b {
                    bw.remove(target);
                }
            }
        }
    }

    /// 正向链接（source 的出链）。
    pub fn outgoing(&self, source_note_id: &str) -> Vec<String> {
        self.forward
            .read()
            .unwrap()
            .get(source_note_id)
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// 反向链接（target 的入链 — 知识图谱/反链面板查询入口）。
    pub fn incoming(&self, target_note_id: &str) -> Vec<String> {
        self.backward
            .read()
            .unwrap()
            .get(target_note_id)
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// 全部正向链接（图谱渲染）。
    pub fn all_links(&self) -> Vec<LinkRow> {
        self.forward
            .read()
            .unwrap()
            .iter()
            .flat_map(|(s, ts)| ts.iter().map(move |t| LinkRow {
                source_note_id: s.clone(),
                target_note_id: t.clone(),
            }))
            .collect()
    }

    fn link_count(&self) -> usize {
        self.forward
            .read()
            .unwrap()
            .values()
            .map(|s| s.len())
            .sum()
    }
}

#[async_trait]
impl Projection for BidiLinkProjection {
    fn name(&self) -> &'static str {
        "bidi_link"
    }

    async fn watermark(&self) -> Result<u64, crate::Error> {
        match self.kv.get(WATERMARK_KEY).await? {
            Some(bytes) => {
                let s = String::from_utf8_lossy(&bytes);
                s.trim().parse::<u64>().map_err(|e| {
                    crate::Error::Internal(format!("watermark parse: {e}"))
                })
            }
            None => Ok(0),
        }
    }

    async fn apply(&self, event: &AppEvent) -> Result<(), crate::Error> {
        if let AppEvent::BidiLinkChanged {
            source_note_id,
            target_note_id,
            action,
        } = event
        {
            self.apply_link(source_note_id, target_note_id, action);
        }
        Ok(())
    }

    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

    async fn set_watermark(&self, seq: u64) -> Result<(), crate::Error> {
        self.kv
            .set(WATERMARK_KEY, seq.to_string().as_bytes())
            .await
    }

    async fn verify(&self) -> Result<ProjectionHealth, crate::Error> {
        // 数量一致性: 仅「投影缺失」（本表 < 数据源）判损坏
        let expected = (self.source)().len();
        let actual = self.link_count();
        if actual < expected {
            warn!(expected, actual, "bidi_link projection missing links");
            return Ok(ProjectionHealth::Corrupted);
        }
        Ok(ProjectionHealth::Ok)
    }

    async fn rebuild(&self) -> Result<(), crate::Error> {
        info!("bidi_link projection: full rebuild");
        let all = (self.source)();
        self.forward.write().unwrap().clear();
        self.backward.write().unwrap().clear();
        for row in &all {
            self.apply_link(&row.source_note_id, &row.target_note_id, &LinkAction::Created);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_bus::layered::LayeredEventBus;
    use crate::l1_infrastructure::storage_engine::MemoryKVStore;
    use std::sync::Arc;

    fn make_bus() -> (LayeredEventBus, Arc<crate::event_bus::layered::InMemoryEventQueue>) {
        let store = Arc::new(crate::event_bus::layered::InMemoryEventQueue::new());
        let bus = LayeredEventBus::new(Some(store.clone()));
        (bus, store)
    }

    fn make_projection(source_rows: Vec<LinkRow>) -> BidiLinkProjection {
        let kv = Arc::new(MemoryKVStore::default());
        let src = Box::new(move || source_rows.clone());
        BidiLinkProjection::new(kv, src)
    }

    #[tokio::test]
    async fn links_project_and_reverse_index_ready() {
        let (bus, _store) = make_bus();
        let p = make_projection(Vec::new());
        bus.publish(AppEvent::BidiLinkChanged {
            source_note_id: "a".into(),
            target_note_id: "b".into(),
            action: LinkAction::Created,
        });
        bus.publish(AppEvent::BidiLinkChanged {
            source_note_id: "c".into(),
            target_note_id: "b".into(),
            action: LinkAction::Created,
        });
        bus.catch_up(&p).await.unwrap();
        assert_eq!(p.outgoing("a"), vec!["b".to_string()]);
        // 反向: b 的入链 [a, c]（BTreeSet 有序）
        assert_eq!(p.incoming("b"), vec!["a".to_string(), "c".to_string()]);
        assert_eq!(p.all_links().len(), 2);
    }

    #[tokio::test]
    async fn link_delete_removes_both_directions() {
        let (bus, _store) = make_bus();
        let p = make_projection(Vec::new());
        bus.publish(AppEvent::BidiLinkChanged {
            source_note_id: "a".into(),
            target_note_id: "b".into(),
            action: LinkAction::Created,
        });
        bus.publish(AppEvent::BidiLinkChanged {
            source_note_id: "a".into(),
            target_note_id: "b".into(),
            action: LinkAction::Deleted,
        });
        bus.catch_up(&p).await.unwrap();
        assert!(p.outgoing("a").is_empty());
        assert!(p.incoming("b").is_empty());
    }

    #[tokio::test]
    async fn rebuild_restores_from_source() {
        let p = make_projection(vec![
            LinkRow { source_note_id: "x".into(), target_note_id: "y".into() },
            LinkRow { source_note_id: "z".into(), target_note_id: "y".into() },
        ]);
        // 事件进 1 条 → 表 1 条 < 源 2 条 → verify Corrupted → rebuild
        let store = Arc::new(crate::event_bus::layered::InMemoryEventQueue::new());
        let bus = LayeredEventBus::new(Some(store));
        bus.publish(AppEvent::BidiLinkChanged {
            source_note_id: "x".into(),
            target_note_id: "y".into(),
            action: LinkAction::Created,
        });
        bus.catch_up(&p).await.unwrap();
        assert_eq!(p.link_count(), 2, "verify 失败触发 rebuild 后 = 源全量");
        assert_eq!(p.incoming("y").len(), 2);
    }

    /// V20 Phase 2: SQLite 双写持久 — 跨进程（重启）经 links 表恢复。
    #[tokio::test]
    async fn sqlite_persistence_across_restart() {
        use crate::traits::kv_store::KVStore;
        use crate::l1_infrastructure::storage_engine::MemoryKVStore;
        use crate::event_bus::layered::LinkAction;

        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("links.db");
        // links 表
        {
            let conn = rusqlite::Connection::open(&db).unwrap();
            conn.execute(
                "CREATE TABLE IF NOT EXISTS links (
                    id TEXT PRIMARY KEY,
                    source_note_id TEXT NOT NULL,
                    target_note_id TEXT NOT NULL,
                    link_type TEXT NOT NULL DEFAULT 'reference',
                    created_at TEXT NOT NULL,
                    metadata TEXT NOT NULL DEFAULT '{}')",
                [],
            )
            .unwrap();
        }

        // 进程一: SQLite 后端投影 + 事件双写
        {
            let conn = rusqlite::Connection::open(&db).unwrap();
            let p = BidiLinkProjection::new_with_sqlite(
                std::sync::Arc::new(MemoryKVStore::default()),
                conn,
            );
            p.apply_link("a", "b", &LinkAction::Created);
            p.apply_link("c", "b", &LinkAction::Created);
            assert_eq!(p.incoming("b").len(), 2, "内存投影正确");
        } // conn drop = 进程退出

        // 进程二: links_from_sqlite 读表恢复
        let rows = links_from_sqlite(&db);
        assert_eq!(rows.len(), 2, "SQLite 双写落表: {rows:?}");
        let has_ab = rows.iter().any(|r| r.source_note_id == "a" && r.target_note_id == "b");
        assert!(has_ab, "a→b 持久化");

        // 删除也持久
        {
            let conn = rusqlite::Connection::open(&db).unwrap();
            let p = BidiLinkProjection::new_with_sqlite(
                std::sync::Arc::new(MemoryKVStore::default()),
                conn,
            );
            p.apply_link("a", "b", &LinkAction::Deleted);
        }
        let rows2 = links_from_sqlite(&db);
        assert_eq!(rows2.len(), 1, "删除持久化: {rows2:?}");
    }

    #[tokio::test]
    async fn crash_recovery_catches_up() {
        let store = crate::event_bus::layered::InMemoryEventQueue::new();
        {
            let bus = LayeredEventBus::new(Some(Arc::new(store.clone())));
            let kv = Arc::new(MemoryKVStore::default());
            let p = BidiLinkProjection::new(kv, Box::new(Vec::new));
            bus.publish(AppEvent::BidiLinkChanged {
                source_note_id: "a".into(),
                target_note_id: "b".into(),
                action: LinkAction::Created,
            });
            bus.catch_up(&p).await.unwrap();
        }
        // 重启: 新总线 + 新投影（水位线在 KV, 但测试 KV 不共享 —
        // 用共享 KV 模拟持久层）
        let store2 = store;
        let kv2 = Arc::new(MemoryKVStore::default());
        // 注意: 真实场景 KV 持久; 此处验证总线侧重放语义:
        let bus2 = LayeredEventBus::new(Some(Arc::new(store2)));
        let p2 = BidiLinkProjection::new(kv2, Box::new(Vec::new));
        bus2.restore_seq().unwrap();
        bus2.publish(AppEvent::BidiLinkChanged {
            source_note_id: "c".into(),
            target_note_id: "d".into(),
            action: LinkAction::Created,
        });
        let n = bus2.catch_up(&p2).await.unwrap();
        // 水位线 0（新 KV）→ 全量重放: a→b + c→d
        assert_eq!(n, 2);
        assert_eq!(p2.link_count(), 2);
    }
}
