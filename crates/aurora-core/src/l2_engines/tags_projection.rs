//! DK-27: 标签投影（tag ↔ note 轻量映射读取源）。
//!
//! 数据源 = KV `note:{id}` NoteRecord.tags（serde default 兼容存量——
//! **存量笔记首次标签变更前投影态为空集，诚实化口径**）。
//! 事件面：NoteCreated（空标签 seed）/ NoteMetadataChanged.tags（终态覆盖）/
//! NoteDeleted（级联清理）。**数据决策：不覆写 apply_batch**——映射为纯内存
//! HashMap 更新，无 tantivy commit 成本（DK-22 98.2% 占比不成立于此），
//! 默认逐事件循环即最优；批量重建 parity 由测试锚定。

use crate::event_bus::layered::AppEvent;
use crate::event_bus::projection::{Projection, ProjectionHealth};
use crate::traits::kv_store::KVStore;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::RwLock;
use tracing::debug;

/// 全量数据源回调（(note_id, tags) 对）。
pub type TagsSource = Box<dyn Fn() -> Vec<(String, Vec<String>)> + Send + Sync>;

const WATERMARK_KEY: &str = "projection.watermark.tags";

/// 单笔记标签行（UI/调试视图）。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TagRow {
    pub note_id: String,
    pub tags: Vec<String>,
}

/// 标签投影。
pub struct TagsProjection {
    rows: RwLock<BTreeMap<String, BTreeSet<String>>>,
    kv: std::sync::Arc<dyn KVStore>,
    source: TagsSource,
}

impl TagsProjection {
    pub fn new(kv: std::sync::Arc<dyn KVStore>, source: TagsSource) -> Self {
        Self {
            rows: RwLock::new(BTreeMap::new()),
            kv,
            source,
        }
    }

    /// 单笔记标签视图（无行 → 空集）。
    pub fn tags_of(&self, note_id: &str) -> Vec<String> {
        self.rows
            .read()
            .unwrap()
            .get(note_id)
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// 按包含/排除语义查询笔记集（include 全含 AND / exclude 任一即否决）。
    pub fn notes_matching(&self, include: &[String], exclude: &[String]) -> Vec<String> {
        let rows = self.rows.read().unwrap();
        rows.iter()
            .filter(|(_, tags)| {
                include.iter().all(|w| tags.contains(w))
                    && !exclude.iter().any(|bad| tags.contains(bad))
            })
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// 投影行数（verify 用）。
    pub fn row_count(&self) -> usize {
        self.rows.read().unwrap().len()
    }

    fn upsert(&self, note_id: &str, tags: Vec<String>) {
        self.rows
            .write()
            .unwrap()
            .insert(note_id.to_string(), tags.into_iter().collect());
    }

    fn seed_empty(&self, note_id: &str) {
        self.rows
            .write()
            .unwrap()
            .entry(note_id.to_string())
            .or_default();
    }
}

#[async_trait::async_trait]
impl Projection for TagsProjection {
    fn name(&self) -> &'static str {
        "tags-index"
    }

    async fn watermark(&self) -> Result<u64, crate::Error> {
        match self.kv.get(WATERMARK_KEY).await? {
            Some(bytes) => {
                let s = String::from_utf8(bytes)
                    .map_err(|e| crate::Error::Internal(format!("watermark utf8: {e}")))?;
                s.parse::<u64>()
                    .map_err(|e| crate::Error::Internal(format!("watermark parse: {e}")))
            }
            None => Ok(0),
        }
    }

    async fn set_watermark(&self, seq: u64) -> Result<(), crate::Error> {
        self.kv.set(WATERMARK_KEY, seq.to_string().as_bytes()).await
    }

    async fn apply(&self, event: &AppEvent) -> Result<(), crate::Error> {
        match event {
            AppEvent::NoteCreated { note_id, .. } => self.seed_empty(note_id),
            AppEvent::NoteMetadataChanged { note_id, changes } => {
                if let Some(tags) = &changes.tags {
                    // 终态覆盖（全量语义——与 set_note_tags 直传一致）
                    self.upsert(note_id, tags.clone());
                }
            }
            AppEvent::NoteDeleted { note_id } => {
                self.rows.write().unwrap().remove(note_id);
            }
            _ => {}
        }
        Ok(())
    }

    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

    async fn verify(&self) -> Result<ProjectionHealth, crate::Error> {
        if let Err(e) = self.watermark().await {
            tracing::warn!(error = %e, "tags projection watermark unreadable");
            return Ok(ProjectionHealth::Corrupted);
        }
        let expected = (self.source)().len();
        let actual = self.row_count();
        if actual < expected {
            tracing::warn!(expected, actual, "tags projection index missing rows");
            return Ok(ProjectionHealth::Corrupted);
        }
        Ok(ProjectionHealth::Ok)
    }

    /// 全量重建：清空并从数据源重放全部映射。
    async fn rebuild(&self) -> Result<(), crate::Error> {
        debug!("tags projection: full rebuild");
        let all = (self.source)();
        let mut rows = self.rows.write().unwrap();
        rows.clear();
        for (id, tags) in all {
            rows.insert(id, tags.into_iter().collect());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_bus::layered::NoteChanges;
    use crate::l1_infrastructure::storage_engine::MemoryKVStore;
    use std::sync::Arc;

    fn proj() -> TagsProjection {
        let kv = Arc::new(MemoryKVStore::default());
        TagsProjection::new(kv, Box::new(Vec::new))
    }

    /// DK-27: 增删改一致性——seed 空 → tags 覆盖 → 删除级联。
    #[tokio::test]
    async fn dk27_tag_projection_lifecycle() {
        let p = proj();
        p.apply(&AppEvent::NoteCreated {
            note_id: "n1".into(),
            title: "T".into(),
            content: String::new(),
        })
        .await
        .unwrap();
        assert!(p.tags_of("n1").is_empty(), "NoteCreated → 空标签 seed");

        p.apply(&AppEvent::NoteMetadataChanged {
            note_id: "n1".into(),
            changes: NoteChanges {
                title: None,
                tags: Some(vec!["rust".into(), "core".into()]),
            },
        })
        .await
        .unwrap();
        assert_eq!(p.tags_of("n1"), vec!["core", "rust"]);

        // 终态覆盖（非增量）——新集合完全替换
        p.apply(&AppEvent::NoteMetadataChanged {
            note_id: "n1".into(),
            changes: NoteChanges {
                title: None,
                tags: Some(vec!["rust".into()]),
            },
        })
        .await
        .unwrap();
        assert_eq!(p.tags_of("n1"), vec!["rust"]);

        // tags: None（仅标题变更）不影响标签态
        p.apply(&AppEvent::NoteMetadataChanged {
            note_id: "n1".into(),
            changes: NoteChanges {
                title: Some("新标题".into()),
                tags: None,
            },
        })
        .await
        .unwrap();
        assert_eq!(p.tags_of("n1"), vec!["rust"]);

        p.apply(&AppEvent::NoteDeleted {
            note_id: "n1".into(),
        })
        .await
        .unwrap();
        assert!(p.tags_of("n1").is_empty());
    }

    /// DK-27: include 全含 AND / exclude 任一否决。
    #[tokio::test]
    async fn dk27_notes_matching_semantics() {
        let p = proj();
        p.upsert("a", vec!["rust".into(), "core".into()]);
        p.upsert("b", vec!["rust".into()]);
        p.upsert("c", vec!["ui".into()]);
        p.upsert("d", vec!["rust".into(), "legacy".into()]);

        assert_eq!(p.notes_matching(&["rust".into()], &[]), vec!["a", "b", "d"]);
        assert_eq!(
            p.notes_matching(&["rust".into(), "core".into()], &[]),
            vec!["a"]
        );
        assert_eq!(
            p.notes_matching(&["rust".into()], &["legacy".into()]),
            vec!["a", "b"]
        );
        assert!(p.notes_matching(&["nope".into()], &[]).is_empty());
    }

    /// DK-27 DoD: 批量重建 parity——rebuild（source 全量）与逐事件终态一致；
    /// 顺带覆盖存量兼容（source 侧 tags 空集行）。
    #[tokio::test]
    async fn dk27_rebuild_parity_vs_incremental() {
        // 事件驱动终态
        let p1 = proj();
        for (id, tags) in [
            ("n1", vec!["rust".to_string(), "core".to_string()]),
            ("n2", vec![]),
        ] {
            p1.apply(&AppEvent::NoteCreated {
                note_id: id.into(),
                title: "T".into(),
                content: String::new(),
            })
            .await
            .unwrap();
            p1.apply(&AppEvent::NoteMetadataChanged {
                note_id: id.into(),
                changes: NoteChanges {
                    title: None,
                    tags: if tags.is_empty() { None } else { Some(tags) },
                },
            })
            .await
            .unwrap();
        }

        // 全量重建终态（同数据）
        let p2 = proj_with_source();
        p2.rebuild().await.unwrap();

        for id in ["n1", "n2"] {
            assert_eq!(
                p1.tags_of(id),
                p2.tags_of(id),
                "apply_batch 逐事件终态与 rebuild 全量终态必须一致（parity）"
            );
        }
    }

    fn proj_with_source() -> TagsProjection {
        let kv = Arc::new(MemoryKVStore::default());
        TagsProjection::new(
            kv,
            Box::new(move || {
                vec![
                    (
                        "n1".to_string(),
                        vec!["rust".to_string(), "core".to_string()],
                    ),
                    ("n2".to_string(), vec![]),
                ]
            }),
        )
    }
}
