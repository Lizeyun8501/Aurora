//! Trait: CrdtEngine — 文档级与块级 CRDT 操作的统一接口
//!
//! V19 §28 原始指定 `async_trait`，本批次 PR 推进异步化迁移。
//! （V26 O-06: `get_history` 占位删除——Loro 事件历史需订阅回调收集,
//! 原实现恒返回空 Vec 且全仓零调用方;真实历史需求由 event_bus 层承担。）

#[cfg(feature = "loro-crdt")]
use crate::l1_infrastructure::crdt::LoroDoc;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeSummary {
    pub doc_id: String,
    pub changes: Vec<BlockChange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockChange {
    pub block_id: String,
    pub op_type: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeResult {
    pub conflicts: Vec<String>,
    pub merged_blocks: Vec<String>,
}

#[async_trait]
pub trait CrdtEngine: Send + Sync {
    #[cfg(feature = "loro-crdt")]
    async fn create_document(&self, doc_id: &str) -> Result<LoroDoc, crate::Error>;
    #[cfg(not(feature = "loro-crdt"))]
    async fn create_document(&self, doc_id: &str) -> Result<(), crate::Error>;
    async fn apply_ops(&self, doc_id: &str, ops: &[u8]) -> Result<ChangeSummary, crate::Error>;
    async fn get_snapshot(&self, doc_id: &str) -> Result<Vec<u8>, crate::Error>;
    async fn merge_branch(
        &self,
        doc_id: &str,
        branch_id: &str,
    ) -> Result<MergeResult, crate::Error>;
}
