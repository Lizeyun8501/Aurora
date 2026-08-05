//! Trait: Storage — 持久化存储接口，支持 KV、关系型、对象存储多种模式
//!
//! V19 §28 原始指定 `async_trait`，本批次 PR 推进异步化迁移。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageQuery {
    pub table: String,
    pub filters: Vec<QueryFilter>,
    pub order_by: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryFilter {
    pub field: String,
    pub op: String,
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    pub data: serde_json::Value,
}

pub enum StorageOp {
    Put { key: String, value: Vec<u8> },
    Delete { key: String },
}

#[async_trait]
pub trait Storage: Send + Sync {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, crate::Error>;
    async fn put(&self, key: &str, value: &[u8]) -> Result<(), crate::Error>;
    async fn delete(&self, key: &str) -> Result<(), crate::Error>;
    async fn query(&self, q: &StorageQuery) -> Result<Vec<Record>, crate::Error>;
    async fn transaction(&self, ops: &[StorageOp]) -> Result<(), crate::Error>;
}
