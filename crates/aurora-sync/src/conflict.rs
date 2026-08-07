//! 冲突解决 (Conflict Resolution)
//!
//! # 三层冲突解决策略
//! 1. CRDT 自动合并：Loro CRDT 在 op 层面保证收敛，无需手动干预。
//!    本模块不重复实现 CRDT 合并算法，仅在 [`ConflictResolution::AutoMerge`]
//!    时记录为「已由 CRDT 解决」。
//! 2. 语义冲突 UI 手动选择：当 CRDT 合并后仍存在语义层面冲突
//!    (如同一图片被替换为不同 URL)，通过 [`SemanticConflict`] 提交给用户手动选择。
//! 3. 分支模式 ([`Branch`])：将冲突版本隔离为独立分支，后续可合并或保留并行。

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

/// 冲突解决策略枚举。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ConflictResolution {
    /// CRDT 自动合并 (默认，无需用户介入)。
    AutoMerge,
    /// 本地版本优先。
    LocalWins,
    /// 远端版本优先。
    RemoteWins,
    /// 按时间戳取最新 (LWW)。
    LastWriteWins,
    /// 用户手动选择。
    ManualSelect,
    /// 创建分支保留双方版本。
    Branch,
}

/// 语义冲突：CRDT 合并后仍需用户介入的冲突。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticConflict {
    pub conflict_id: String,
    pub doc_id: String,
    pub block_id: String,
    /// 冲突字段路径。
    pub field: String,
    /// 本地候选值。
    pub local_value: serde_json::Value,
    /// 远端候选值。
    pub remote_value: serde_json::Value,
    /// 用户选择的解决策略。
    pub resolution: Option<ConflictResolution>,
    /// 已选定的最终值。
    pub resolved_value: Option<serde_json::Value>,
    /// 本地候选值的最后修改时间（LWW 判定依据；缺省则无法比较）。
    #[serde(default)]
    pub local_updated_at: Option<chrono::DateTime<chrono::Utc>>,
    /// 远端候选值的最后修改时间（LWW 判定依据；缺省则无法比较）。
    #[serde(default)]
    pub remote_updated_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl SemanticConflict {
    pub fn new(
        doc_id: impl Into<String>,
        block_id: impl Into<String>,
        field: impl Into<String>,
        local_value: serde_json::Value,
        remote_value: serde_json::Value,
    ) -> Self {
        Self {
            conflict_id: uuid::Uuid::new_v4().to_string(),
            doc_id: doc_id.into(),
            block_id: block_id.into(),
            field: field.into(),
            local_value,
            remote_value,
            resolution: None,
            resolved_value: None,
            local_updated_at: None,
            remote_updated_at: None,
        }
    }

    /// 附带双侧修改时间（供 LastWriteWins 真实按时间戳比较）。
    pub fn with_timestamps(
        mut self,
        local_updated_at: Option<chrono::DateTime<chrono::Utc>>,
        remote_updated_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Self {
        self.local_updated_at = local_updated_at;
        self.remote_updated_at = remote_updated_at;
        self
    }

    /// 是否已解决。
    pub fn is_resolved(&self) -> bool {
        self.resolution.is_some() && self.resolved_value.is_some()
    }
}

/// 分支：保留冲突双方的独立版本线。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Branch {
    pub branch_id: String,
    pub doc_id: String,
    pub name: String,
    /// 分支基准版本向量。
    pub base_vv: HashMap<String, u64>,
    /// 分支头部版本向量。
    pub head_vv: HashMap<String, u64>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub archived: bool,
}

impl Branch {
    pub fn new(
        doc_id: impl Into<String>,
        name: impl Into<String>,
        base_vv: HashMap<String, u64>,
    ) -> Self {
        let head_vv = base_vv.clone();
        Self {
            branch_id: uuid::Uuid::new_v4().to_string(),
            doc_id: doc_id.into(),
            name: name.into(),
            base_vv,
            head_vv,
            created_at: chrono::Utc::now(),
            archived: false,
        }
    }

    /// 推进分支头部版本。
    pub fn advance(&mut self, peer: impl Into<String>, seq: u64) {
        let entry = self.head_vv.entry(peer.into()).or_insert(0);
        if seq > *entry {
            *entry = seq;
        }
    }

    /// 归档分支 (不再参与合并)。
    pub fn archive(&mut self) {
        self.archived = true;
    }

    /// 是否已领先基准版本。
    pub fn has_advanced(&self) -> bool {
        self.head_vv != self.base_vv
    }
}

/// 冲突解决器。
pub struct ConflictResolver {
    conflicts: Arc<RwLock<HashMap<String, SemanticConflict>>>,
    branches: Arc<RwLock<HashMap<String, Branch>>>,
    default_strategy: ConflictResolution,
}

impl ConflictResolver {
    pub fn new(default_strategy: ConflictResolution) -> Self {
        Self {
            conflicts: Arc::new(RwLock::new(HashMap::new())),
            branches: Arc::new(RwLock::new(HashMap::new())),
            default_strategy,
        }
    }

    /// 登记一个语义冲突。
    pub fn register(&self, conflict: SemanticConflict) -> String {
        let id = conflict.conflict_id.clone();
        debug!("conflict registered: {} field={}", id, conflict.field);
        self.conflicts.write().insert(id.clone(), conflict);
        id
    }

    /// 列出未解决的冲突。
    pub fn pending_conflicts(&self) -> Vec<SemanticConflict> {
        self.conflicts
            .read()
            .values()
            .filter(|c| !c.is_resolved())
            .cloned()
            .collect()
    }

    /// 列出全部冲突。
    pub fn all_conflicts(&self) -> Vec<SemanticConflict> {
        self.conflicts.read().values().cloned().collect()
    }

    /// 解决冲突：按指定策略选择值。
    pub fn resolve(
        &self,
        conflict_id: &str,
        resolution: ConflictResolution,
    ) -> crate::Result<serde_json::Value> {
        let value = {
            let mut conflicts = self.conflicts.write();
            let conflict = conflicts.get_mut(conflict_id).ok_or_else(|| {
                crate::Error::Conflict(format!("conflict not found: {}", conflict_id))
            })?;
            let value = match resolution {
                ConflictResolution::LocalWins => conflict.local_value.clone(),
                ConflictResolution::RemoteWins => conflict.remote_value.clone(),
                ConflictResolution::LastWriteWins => {
                    // 真实 LWW：比较双侧修改时间，取较新者；时间戳相等时
                    // 本地优先（确定性）。任一侧缺时间戳则无法判定“最新”，
                    // 回退为远端优先并告警（此前无条件取远端，并非 LWW）。
                    match (conflict.local_updated_at, conflict.remote_updated_at) {
                        (Some(local_ts), Some(remote_ts)) => {
                            if local_ts >= remote_ts {
                                conflict.local_value.clone()
                            } else {
                                conflict.remote_value.clone()
                            }
                        }
                        _ => {
                            warn!(
                                conflict_id = %conflict.conflict_id,
                                "LastWriteWins without timestamps; falling back to remote"
                            );
                            conflict.remote_value.clone()
                        }
                    }
                }
                ConflictResolution::AutoMerge => conflict.local_value.clone(),
                ConflictResolution::ManualSelect => {
                    return Err(crate::Error::Conflict(
                        "ManualSelect requires explicit value via resolve_with_value".to_string(),
                    ));
                }
                ConflictResolution::Branch => conflict.local_value.clone(),
            };
            conflict.resolution = Some(resolution);
            conflict.resolved_value = Some(value.clone());
            value
        };

        // 分支策略：创建分支保留双方版本
        if resolution == ConflictResolution::Branch {
            let conflicts = self.conflicts.read();
            if let Some(conflict) = conflicts.get(conflict_id) {
                let mut base_vv = HashMap::new();
                base_vv.insert("local".to_string(), 1);
                let mut branch = Branch::new(
                    conflict.doc_id.clone(),
                    format!(
                        "conflict-{}",
                        &conflict.conflict_id[..8.min(conflict.conflict_id.len())]
                    ),
                    base_vv,
                );
                branch.advance("local", 1);
                branch.advance("remote", 1);
                self.branches
                    .write()
                    .insert(branch.branch_id.clone(), branch);
            }
        }

        info!("conflict resolved: {} via {:?}", conflict_id, resolution);
        Ok(value)
    }

    /// 手动选择指定值解决冲突。
    pub fn resolve_with_value(
        &self,
        conflict_id: &str,
        value: serde_json::Value,
    ) -> crate::Result<()> {
        let mut conflicts = self.conflicts.write();
        let conflict = conflicts.get_mut(conflict_id).ok_or_else(|| {
            crate::Error::Conflict(format!("conflict not found: {}", conflict_id))
        })?;
        conflict.resolution = Some(ConflictResolution::ManualSelect);
        conflict.resolved_value = Some(value);
        Ok(())
    }

    /// 获取默认策略。
    pub fn default_strategy(&self) -> ConflictResolution {
        self.default_strategy
    }

    /// 列出所有分支。
    pub fn branches(&self) -> Vec<Branch> {
        self.branches.read().values().cloned().collect()
    }

    /// 归档分支。
    pub fn archive_branch(&self, branch_id: &str) -> crate::Result<()> {
        let mut branches = self.branches.write();
        let branch = branches
            .get_mut(branch_id)
            .ok_or_else(|| crate::Error::NotFound(format!("branch not found: {}", branch_id)))?;
        branch.archive();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_conflict() -> SemanticConflict {
        SemanticConflict::new(
            "doc1",
            "block-1",
            "image_url",
            serde_json::json!("https://local/img.png"),
            serde_json::json!("https://remote/img.png"),
        )
    }

    #[test]
    fn test_semantic_conflict_new_unresolved() {
        let c = make_conflict();
        assert!(!c.is_resolved());
        assert_eq!(c.field, "image_url");
        assert_eq!(c.local_value, serde_json::json!("https://local/img.png"));
        assert_eq!(c.remote_value, serde_json::json!("https://remote/img.png"));
    }

    #[test]
    fn test_conflict_resolve_local_wins() {
        let resolver = ConflictResolver::new(ConflictResolution::AutoMerge);
        let id = resolver.register(make_conflict());
        let value = resolver
            .resolve(&id, ConflictResolution::LocalWins)
            .expect("resolve");
        assert_eq!(value, serde_json::json!("https://local/img.png"));
        // 已解决，不再出现在 pending
        assert_eq!(resolver.pending_conflicts().len(), 0);
    }

    #[test]
    fn test_conflict_resolve_remote_wins() {
        let resolver = ConflictResolver::new(ConflictResolution::AutoMerge);
        let id = resolver.register(make_conflict());
        let value = resolver
            .resolve(&id, ConflictResolution::RemoteWins)
            .expect("resolve");
        assert_eq!(value, serde_json::json!("https://remote/img.png"));
    }

    #[test]
    fn test_conflict_resolve_with_value_manual() {
        let resolver = ConflictResolver::new(ConflictResolution::AutoMerge);
        let id = resolver.register(make_conflict());
        resolver
            .resolve_with_value(&id, serde_json::json!("https://custom/img.png"))
            .expect("resolve");
        let conflicts = resolver.all_conflicts();
        assert_eq!(conflicts.len(), 1);
        assert!(conflicts[0].is_resolved());
        assert_eq!(
            conflicts[0].resolved_value,
            Some(serde_json::json!("https://custom/img.png"))
        );
        assert_eq!(
            conflicts[0].resolution,
            Some(ConflictResolution::ManualSelect)
        );
    }

    #[test]
    fn test_conflict_resolve_creates_branch() {
        let resolver = ConflictResolver::new(ConflictResolution::AutoMerge);
        let id = resolver.register(make_conflict());
        let _ = resolver
            .resolve(&id, ConflictResolution::Branch)
            .expect("resolve");
        let branches = resolver.branches();
        assert_eq!(branches.len(), 1);
        assert!(branches[0].has_advanced());
        assert_eq!(branches[0].head_vv.get("local").copied(), Some(1));
        assert_eq!(branches[0].head_vv.get("remote").copied(), Some(1));
    }

    #[test]
    fn test_branch_advance_and_archive() {
        let resolver = ConflictResolver::new(ConflictResolution::AutoMerge);
        let id = resolver.register(make_conflict());
        let _ = resolver.resolve(&id, ConflictResolution::Branch).unwrap();
        let branch_id = resolver.branches()[0].branch_id.clone();
        resolver.archive_branch(&branch_id).expect("archive");
        let branches = resolver.branches();
        assert!(branches[0].archived);
    }

    #[test]
    fn test_pending_conflicts_filter() {
        let resolver = ConflictResolver::new(ConflictResolution::AutoMerge);
        let id1 = resolver.register(make_conflict());
        let _id2 = resolver.register(SemanticConflict::new(
            "doc2",
            "b2",
            "f2",
            serde_json::json!(1),
            serde_json::json!(2),
        ));
        assert_eq!(resolver.pending_conflicts().len(), 2);
        resolver
            .resolve(&id1, ConflictResolution::LocalWins)
            .unwrap();
        assert_eq!(resolver.pending_conflicts().len(), 1);
    }

    #[test]
    fn test_resolve_unknown_conflict_errors() {
        let resolver = ConflictResolver::new(ConflictResolution::AutoMerge);
        let result = resolver.resolve("missing", ConflictResolution::LocalWins);
        assert!(result.is_err());
    }

    #[test]
    fn test_manual_select_requires_value() {
        let resolver = ConflictResolver::new(ConflictResolution::AutoMerge);
        let id = resolver.register(make_conflict());
        // 直接用 ManualSelect 调 resolve 应失败
        let result = resolver.resolve(&id, ConflictResolution::ManualSelect);
        assert!(result.is_err());
    }

    #[test]
    fn test_default_strategy() {
        let resolver = ConflictResolver::new(ConflictResolution::LastWriteWins);
        assert_eq!(
            resolver.default_strategy(),
            ConflictResolution::LastWriteWins
        );
    }

    #[test]
    fn test_lww_prefers_newer_timestamp() {
        use chrono::TimeZone;
        let older = chrono::Utc.with_ymd_and_hms(2026, 8, 1, 0, 0, 0).unwrap();
        let newer = chrono::Utc.with_ymd_and_hms(2026, 8, 7, 0, 0, 0).unwrap();
        let resolver = ConflictResolver::new(ConflictResolution::LastWriteWins);

        // 本地更新 → 取本地
        let id = resolver.register(make_conflict().with_timestamps(Some(newer), Some(older)));
        let value = resolver
            .resolve(&id, ConflictResolution::LastWriteWins)
            .expect("resolve");
        assert_eq!(value, serde_json::json!("https://local/img.png"));

        // 远端更新 → 取远端
        let id = resolver.register(make_conflict().with_timestamps(Some(older), Some(newer)));
        let value = resolver
            .resolve(&id, ConflictResolution::LastWriteWins)
            .expect("resolve");
        assert_eq!(value, serde_json::json!("https://remote/img.png"));
    }

    #[test]
    fn test_lww_without_timestamps_falls_back_to_remote() {
        let resolver = ConflictResolver::new(ConflictResolution::LastWriteWins);
        let id = resolver.register(make_conflict());
        let value = resolver
            .resolve(&id, ConflictResolution::LastWriteWins)
            .expect("resolve");
        assert_eq!(value, serde_json::json!("https://remote/img.png"));
    }
}
