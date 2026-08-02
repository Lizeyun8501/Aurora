//! P2P 同步 (基于 iroh)
//!
//! 提供点对点 (P2P) 数据同步与节点发现能力，支持去中心化的多端同步。
//! 底层使用 [iroh](https://iroh.computer) 实现。

use std::collections::HashMap;
use std::sync::Mutex;

use crate::traits::sync_target::{Connection, DocSet, Endpoint, SyncEvent, SyncProtocol, SyncReport, SyncTarget};

/// 基于 iroh 的 P2P 同步目标实现。
pub struct IrohSyncTarget {
    connections: Mutex<HashMap<String, Connection>>,
    callback: Mutex<Option<Box<dyn Fn(SyncEvent) + Send + Sync>>>,
}

impl IrohSyncTarget {
    /// 创建新的 iroh 同步目标实例。
    pub fn new() -> Self {
        Self {
            connections: Mutex::new(HashMap::new()),
            callback: Mutex::new(None),
        }
    }
}

impl Default for IrohSyncTarget {
    fn default() -> Self {
        Self::new()
    }
}

impl SyncTarget for IrohSyncTarget {
    fn connect(&mut self, endpoint: &Endpoint) -> Result<Connection, crate::Error> {
        if !matches!(endpoint.protocol, SyncProtocol::Iroh | SyncProtocol::Quic) {
            return Err(crate::Error::InvalidInput(format!(
                "IrohSyncTarget does not support protocol {:?}",
                endpoint.protocol
            )));
        }
        let conn = Connection {
            id: uuid::Uuid::new_v4().to_string(),
            endpoint: endpoint.clone(),
        };
        self.connections
            .lock()
            .map_err(|_| crate::Error::Internal("iroh connections mutex poisoned".to_string()))?
            .insert(conn.id.clone(), conn.clone());
        if let Ok(cb) = self.callback.lock() {
            if let Some(ref callback) = *cb {
                callback(SyncEvent::Connected {
                    conn_id: conn.id.clone(),
                });
            }
        }
        Ok(conn)
    }

    fn sync(&self, conn: &Connection, doc_set: &DocSet) -> Result<SyncReport, crate::Error> {
        let connections = self
            .connections
            .lock()
            .map_err(|_| crate::Error::Internal("iroh connections mutex poisoned".to_string()))?;
        if !connections.contains_key(&conn.id) {
            return Err(crate::Error::NotFound(format!(
                "connection not found: {}",
                conn.id
            )));
        }
        tracing::info!(
            "iroh sync: conn={}, docs={:?}",
            conn.id,
            doc_set.doc_ids
        );
        // TODO: 接入 iroh 真实的文档同步协议 (iroh-docs / iroh-blobs)。
        Ok(SyncReport {
            sent_ops: 0,
            received_ops: 0,
            duration_ms: 0,
        })
    }

    fn watch(&self, callback: Box<dyn Fn(SyncEvent) + Send + Sync>) {
        if let Ok(mut cb) = self.callback.lock() {
            *cb = Some(callback);
        }
    }

    fn disconnect(&self, conn: &Connection) -> Result<(), crate::Error> {
        let mut connections = self
            .connections
            .lock()
            .map_err(|_| crate::Error::Internal("iroh connections mutex poisoned".to_string()))?;
        connections.remove(&conn.id);
        if let Ok(cb) = self.callback.lock() {
            if let Some(ref callback) = *cb {
                callback(SyncEvent::Disconnected {
                    conn_id: conn.id.clone(),
                });
            }
        }
        Ok(())
    }
}

/// 基于 WebSocket 的同步目标实现。
pub struct WebSocketSyncTarget {
    connections: Mutex<HashMap<String, Connection>>,
    callback: Mutex<Option<Box<dyn Fn(SyncEvent) + Send + Sync>>>,
}

impl WebSocketSyncTarget {
    /// 创建新的 WebSocket 同步目标实例。
    pub fn new() -> Self {
        Self {
            connections: Mutex::new(HashMap::new()),
            callback: Mutex::new(None),
        }
    }
}

impl Default for WebSocketSyncTarget {
    fn default() -> Self {
        Self::new()
    }
}

impl SyncTarget for WebSocketSyncTarget {
    fn connect(&mut self, endpoint: &Endpoint) -> Result<Connection, crate::Error> {
        if endpoint.protocol != SyncProtocol::WebSocket {
            return Err(crate::Error::InvalidInput(format!(
                "WebSocketSyncTarget does not support protocol {:?}",
                endpoint.protocol
            )));
        }
        let conn = Connection {
            id: uuid::Uuid::new_v4().to_string(),
            endpoint: endpoint.clone(),
        };
        self.connections
            .lock()
            .map_err(|_| crate::Error::Internal("ws connections mutex poisoned".to_string()))?
            .insert(conn.id.clone(), conn.clone());
        if let Ok(cb) = self.callback.lock() {
            if let Some(ref callback) = *cb {
                callback(SyncEvent::Connected {
                    conn_id: conn.id.clone(),
                });
            }
        }
        Ok(conn)
    }

    fn sync(&self, conn: &Connection, doc_set: &DocSet) -> Result<SyncReport, crate::Error> {
        let connections = self
            .connections
            .lock()
            .map_err(|_| crate::Error::Internal("ws connections mutex poisoned".to_string()))?;
        if !connections.contains_key(&conn.id) {
            return Err(crate::Error::NotFound(format!(
                "connection not found: {}",
                conn.id
            )));
        }
        tracing::info!(
            "websocket sync: conn={}, docs={:?}",
            conn.id,
            doc_set.doc_ids
        );
        // TODO: 接入 tokio-tungstenite 或 async-tungstenite 实现真实 WebSocket 同步。
        Ok(SyncReport {
            sent_ops: 0,
            received_ops: 0,
            duration_ms: 0,
        })
    }

    fn watch(&self, callback: Box<dyn Fn(SyncEvent) + Send + Sync>) {
        if let Ok(mut cb) = self.callback.lock() {
            *cb = Some(callback);
        }
    }

    fn disconnect(&self, conn: &Connection) -> Result<(), crate::Error> {
        let mut connections = self
            .connections
            .lock()
            .map_err(|_| crate::Error::Internal("ws connections mutex poisoned".to_string()))?;
        connections.remove(&conn.id);
        if let Ok(cb) = self.callback.lock() {
            if let Some(ref callback) = *cb {
                callback(SyncEvent::Disconnected {
                    conn_id: conn.id.clone(),
                });
            }
        }
        Ok(())
    }
}

/// 基于局域网 (LAN) 的同步目标实现。
///
/// 通过本地网络广播或多播发现邻近节点，实现局域网内高速同步。
pub struct LanSyncTarget {
    connections: Mutex<HashMap<String, Connection>>,
    callback: Mutex<Option<Box<dyn Fn(SyncEvent) + Send + Sync>>>,
}

impl LanSyncTarget {
    /// 创建新的 LAN 同步目标实例。
    pub fn new() -> Self {
        Self {
            connections: Mutex::new(HashMap::new()),
            callback: Mutex::new(None),
        }
    }
}

impl Default for LanSyncTarget {
    fn default() -> Self {
        Self::new()
    }
}

impl SyncTarget for LanSyncTarget {
    fn connect(&mut self, endpoint: &Endpoint) -> Result<Connection, crate::Error> {
        let conn = Connection {
            id: uuid::Uuid::new_v4().to_string(),
            endpoint: endpoint.clone(),
        };
        self.connections
            .lock()
            .map_err(|_| crate::Error::Internal("lan connections mutex poisoned".to_string()))?
            .insert(conn.id.clone(), conn.clone());
        if let Ok(cb) = self.callback.lock() {
            if let Some(ref callback) = *cb {
                callback(SyncEvent::Connected {
                    conn_id: conn.id.clone(),
                });
            }
        }
        Ok(conn)
    }

    fn sync(&self, conn: &Connection, doc_set: &DocSet) -> Result<SyncReport, crate::Error> {
        let connections = self
            .connections
            .lock()
            .map_err(|_| crate::Error::Internal("lan connections mutex poisoned".to_string()))?;
        if !connections.contains_key(&conn.id) {
            return Err(crate::Error::NotFound(format!(
                "connection not found: {}",
                conn.id
            )));
        }
        tracing::info!("lan sync: conn={}, docs={:?}", conn.id, doc_set.doc_ids);
        // TODO: 接入本地网络发现协议 (如 mDNS) 与直连传输。
        Ok(SyncReport {
            sent_ops: 0,
            received_ops: 0,
            duration_ms: 0,
        })
    }

    fn watch(&self, callback: Box<dyn Fn(SyncEvent) + Send + Sync>) {
        if let Ok(mut cb) = self.callback.lock() {
            *cb = Some(callback);
        }
    }

    fn disconnect(&self, conn: &Connection) -> Result<(), crate::Error> {
        let mut connections = self
            .connections
            .lock()
            .map_err(|_| crate::Error::Internal("lan connections mutex poisoned".to_string()))?;
        connections.remove(&conn.id);
        if let Ok(cb) = self.callback.lock() {
            if let Some(ref callback) = *cb {
                callback(SyncEvent::Disconnected {
                    conn_id: conn.id.clone(),
                });
            }
        }
        Ok(())
    }
}
