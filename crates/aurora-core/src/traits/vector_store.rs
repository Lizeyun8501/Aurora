//! Trait: VectorStore — 向量存储与相似度搜索接口
//!
//! V19 §28 原始指定 `async_trait`，本批次 PR 推进异步化迁移。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub id: String,
    pub score: f32,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryFilter {
    pub field: String,
    pub op: String,
    pub value: serde_json::Value,
}

#[async_trait]
pub trait VectorStore: Send + Sync {
    async fn add(
        &self,
        id: &str,
        vector: &[f32],
        metadata: &serde_json::Value,
    ) -> Result<(), crate::Error>;
    async fn search(
        &self,
        query: &[f32],
        top_k: usize,
        filter: Option<&QueryFilter>,
    ) -> Result<Vec<SearchResult>, crate::Error>;
    async fn delete(&self, id: &str) -> Result<(), crate::Error>;
    async fn hybrid_search(
        &self,
        text_query: &str,
        vector: &[f32],
        alpha: f32,
    ) -> Result<Vec<SearchResult>, crate::Error>;
}
