//! Trait 2: SyncTarget — 同步目标的抽象，支持 P2P、云端、局域网多种同步模式

use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct Endpoint {
    pub url: String,
    pub protocol: SyncProtocol,
}

#[derive(Debug, Clone)]
pub enum SyncProtocol {
    Iroh,
    WebSocket,
    Quic,
}

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

pub trait SyncTarget: Send + Sync {
    fn connect(&mut self, endpoint: &Endpoint) -> Result<Connection, crate::Error>;
    fn sync(&self, conn: &Connection, doc_set: &DocSet) -> Result<SyncReport, crate::Error>;
    fn watch(&self, callback: Box<dyn Fn(SyncEvent) + Send + Sync>);
    fn disconnect(&self, conn: &Connection) -> Result<(), crate::Error>;
}
