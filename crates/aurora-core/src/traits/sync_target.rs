//! Trait: SyncTarget — 同步目标的抽象，支持 P2P、云端、局域网多种同步模式
//!
//! V19 §28 原始指定 `async_trait`，本批次 PR 推进异步化迁移。
//! 回调注册方法 `watch` 保持同步签名（fire-and-forget）。

use async_trait::async_trait;

#[derive(Debug, Clone)]
pub struct Endpoint {
    pub url: String,
    pub protocol: SyncProtocol,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SyncProtocol {
    Iroh,
    WebSocket,
    Quic,
}

#[derive(Debug, Clone)]
pub struct Connection {
    pub id: String,
    pub endpoint: Endpoint,
}

#[derive(Debug, Clone)]
pub struct DocSet {
    pub doc_ids: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SyncReport {
    pub sent_ops: usize,
    pub received_ops: usize,
    pub duration_ms: u64,
}

#[derive(Debug, Clone)]
pub enum SyncEvent {
    Connected { conn_id: String },
    Disconnected { conn_id: String },
    Progress { progress: f32 },
    Error { message: String },
}

#[async_trait]
pub trait SyncTarget: Send + Sync {
    async fn connect(&mut self, endpoint: &Endpoint) -> Result<Connection, crate::Error>;
    async fn sync(&self, conn: &Connection, doc_set: &DocSet) -> Result<SyncReport, crate::Error>;
    /// 注册同步事件回调（fire-and-forget，保持同步签名）。
    fn watch(&self, callback: Box<dyn Fn(SyncEvent) + Send + Sync>);
    async fn disconnect(&self, conn: &Connection) -> Result<(), crate::Error>;
}
