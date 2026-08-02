//! Trait 3: VectorStore — 向量存储与相似度搜索接口

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

pub trait VectorStore: Send + Sync {
    fn add(&self, id: &str, vector: &[f32], metadata: &serde_json::Value) -> Result<(), crate::Error>;
    fn search(&self, query: &[f32], top_k: usize, filter: Option<&QueryFilter>) -> Result<Vec<SearchResult>, crate::Error>;
    fn delete(&self, id: &str) -> Result<(), crate::Error>;
    fn hybrid_search(&self, text_query: &str, vector: &[f32], alpha: f32) -> Result<Vec<SearchResult>, crate::Error>;
}
