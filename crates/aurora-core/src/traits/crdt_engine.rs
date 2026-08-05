//! Trait: CrdtEngine — 文档级与块级 CRDT 操作的统一接口
//!
//! V19 §28 原始指定 `async_trait`，本批次 PR 推进异步化迁移。
//! 纯计算方法 `get_history` 保持同步签名。

use async_trait::async_trait;
use crate::l1_infrastructure::crdt::LoroDoc;
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
pub struct Event {
    pub event_id: String,
    pub block_id: String,
    pub op_type: String,
    pub payload: serde_json::Value,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeResult {
    pub conflicts: Vec<String>,
    pub merged_blocks: Vec<String>,
}

pub type Timestamp = u64;

#[async_trait]
pub trait CrdtEngine: Send + Sync {
    async fn create_document(&self, doc_id: &str) -> Result<LoroDoc, crate::Error>;
    async fn apply_ops(&self, doc_id: &str, ops: &[u8]) -> Result<ChangeSummary, crate::Error>;
    async fn get_snapshot(&self, doc_id: &str) -> Result<Vec<u8>, crate::Error>;
    /// 纯计算方法，保持同步签名。
    fn get_history(&self, doc_id: &str, since: Option<Timestamp>) -> Vec<Event>;
    async fn merge_branch(&self, doc_id: &str, branch_id: &str) -> Result<MergeResult, crate::Error>;
}
