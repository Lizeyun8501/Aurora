//! P2P 同步 (基于 iroh)
//!
//! 提供点对点 (P2P) 数据同步与节点发现能力，支持去中心化的多端同步。
//! 底层使用 [iroh](https://iroh.computer) 实现。

use async_trait::async_trait;

/// 同步事件回调（对端状态变化通知）。
type SyncEventCallback = Box<dyn Fn(SyncEvent) + Send + Sync>;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::traits::sync_target::{
    Connection, DocSet, Endpoint, SyncEvent, SyncProtocol, SyncReport, SyncTarget,
};

/// 基于 iroh 的 P2P 同步目标实现。
pub struct IrohSyncTarget {
    connections: Mutex<HashMap<String, Connection>>,
    callback: Mutex<Option<SyncEventCallback>>,
    /// V23-I3 / T3: 对端传输数据面（None = sync 大声失败）
    transport: Mutex<Option<Arc<dyn crate::traits::sync_target::PeerTransport>>>,
    /// 本地 oplog 导出/远端合并钩子
    hooks: Mutex<Option<Arc<crate::traits::sync_target::SyncHooks>>>,
}

impl IrohSyncTarget {
    /// 创建新的 iroh 同步目标实例。
    pub fn new() -> Self {
        Self {
            connections: Mutex::new(HashMap::new()),
            callback: Mutex::new(None),
            transport: Mutex::new(None),
            hooks: Mutex::new(None),
        }
    }

    /// 注入对端传输（T3 — 注入后 sync 走真搬运往返）。
    pub fn set_transport(&self, t: Arc<dyn crate::traits::sync_target::PeerTransport>) {
        *self.transport.lock().unwrap() = Some(t);
    }

    /// 注入本地导出/远端合并钩子。
    pub fn set_hooks(&self, h: Arc<crate::traits::sync_target::SyncHooks>) {
        *self.hooks.lock().unwrap() = Some(h);
    }
}

impl Default for IrohSyncTarget {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SyncTarget for IrohSyncTarget {
    async fn connect(&mut self, endpoint: &Endpoint) -> Result<Connection, crate::Error> {
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

    async fn sync(&self, _conn: &Connection, doc_set: &DocSet) -> Result<SyncReport, crate::Error> {
        // V23-I3 / T3: 真搬运往返 — 无传输/无钩子大声失败（零静默占位）。
        // 此前恒返零报告 → SyncRouter 永远认为同步成功 → 降级链永不触发。
        let transport = self
            .transport
            .lock()
            .map_err(|_| crate::Error::Internal("iroh transport mutex poisoned".to_string()))?
            .clone()
            .ok_or_else(|| {
                crate::Error::Internal(
                    "iroh sync: no peer transport configured — sync fails loudly (T3)".to_string(),
                )
            })?;
        let hooks = self
            .hooks
            .lock()
            .map_err(|_| crate::Error::Internal("iroh hooks mutex poisoned".to_string()))?
            .clone()
            .ok_or_else(|| {
                crate::Error::Internal("iroh sync: no sync hooks configured (T3)".to_string())
            })?;

        let started = std::time::Instant::now();
        let mut sent_ops = 0usize;
        let mut received_ops = 0usize;
        for doc_id in &doc_set.doc_ids {
            // push: 本地 oplog → 对端
            let local = (hooks.export)(doc_id)?;
            if !local.is_empty() {
                sent_ops += transport.push(doc_id, local).await?;
            }
            // pull: 对端增量 → 本地合并
            let remote = transport.pull(doc_id).await?;
            if !remote.is_empty() {
                (hooks.merge)(doc_id, remote)?;
                received_ops += 1;
            }
        }
        Ok(SyncReport {
            sent_ops,
            received_ops,
            duration_ms: started.elapsed().as_millis() as u64,
        })
    }

    fn watch(&self, callback: Box<dyn Fn(SyncEvent) + Send + Sync>) {
        if let Ok(mut cb) = self.callback.lock() {
            *cb = Some(callback);
        }
    }

    async fn disconnect(&self, conn: &Connection) -> Result<(), crate::Error> {
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
    callback: Mutex<Option<SyncEventCallback>>,
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

#[async_trait]
impl SyncTarget for WebSocketSyncTarget {
    async fn connect(&mut self, endpoint: &Endpoint) -> Result<Connection, crate::Error> {
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

    async fn sync(&self, conn: &Connection, doc_set: &DocSet) -> Result<SyncReport, crate::Error> {
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
        // V23-I3 / T3 零静默占位: WebSocket 传输未实装 — 大声失败
        // （生产接 tokio-tungstenite + PeerTransport 适配后消除）。
        return Err(crate::Error::Internal(
            "websocket sync: transport not implemented — fails loudly (T3)".to_string(),
        ));
        #[allow(unreachable_code)]
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

    async fn disconnect(&self, conn: &Connection) -> Result<(), crate::Error> {
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
    callback: Mutex<Option<SyncEventCallback>>,
    /// V23-I3 / T3: 对端传输数据面（None = sync 大声失败）
    transport: Mutex<Option<Arc<dyn crate::traits::sync_target::PeerTransport>>>,
    /// 本地 oplog 导出/远端合并钩子
    hooks: Mutex<Option<Arc<crate::traits::sync_target::SyncHooks>>>,
}

impl LanSyncTarget {
    /// 创建新的 LAN 同步目标实例。
    pub fn new() -> Self {
        Self {
            connections: Mutex::new(HashMap::new()),
            callback: Mutex::new(None),
            transport: Mutex::new(None),
            hooks: Mutex::new(None),
        }
    }

    /// 注入对端传输（T3）。
    pub fn set_transport(&self, t: Arc<dyn crate::traits::sync_target::PeerTransport>) {
        *self.transport.lock().unwrap() = Some(t);
    }

    /// 注入本地导出/远端合并钩子。
    pub fn set_hooks(&self, h: Arc<crate::traits::sync_target::SyncHooks>) {
        *self.hooks.lock().unwrap() = Some(h);
    }
}

impl Default for LanSyncTarget {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SyncTarget for LanSyncTarget {
    async fn connect(&mut self, endpoint: &Endpoint) -> Result<Connection, crate::Error> {
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

    async fn sync(&self, conn: &Connection, doc_set: &DocSet) -> Result<SyncReport, crate::Error> {
        // 连接存在性检查限定作用域 — guard 严禁跨 await（死锁教训）
        {
            let connections = self.connections.lock().map_err(|_| {
                crate::Error::Internal("lan connections mutex poisoned".to_string())
            })?;
            if !connections.contains_key(&conn.id) {
                return Err(crate::Error::NotFound(format!(
                    "connection not found: {}",
                    conn.id
                )));
            }
        }
        // V23-I3 / T3: 真搬运往返（与 IrohSyncTarget 同构 — 传输注入 +
        // 导出/合并钩子; 无配置大声失败 → SyncRouter 正常触发降级）。
        let transport = self
            .transport
            .lock()
            .map_err(|_| crate::Error::Internal("lan transport mutex poisoned".to_string()))?
            .clone()
            .ok_or_else(|| {
                crate::Error::Internal(
                    "lan sync: no peer transport configured — sync fails loudly (T3)".to_string(),
                )
            })?;
        let hooks = self
            .hooks
            .lock()
            .map_err(|_| crate::Error::Internal("lan hooks mutex poisoned".to_string()))?
            .clone()
            .ok_or_else(|| {
                crate::Error::Internal("lan sync: no sync hooks configured (T3)".to_string())
            })?;
        let started = std::time::Instant::now();
        let mut sent_ops = 0usize;
        let mut received_ops = 0usize;
        for doc_id in &doc_set.doc_ids {
            let local = (hooks.export)(doc_id)?;
            if !local.is_empty() {
                sent_ops += transport.push(doc_id, local).await?;
            }
            let remote = transport.pull(doc_id).await?;
            if !remote.is_empty() {
                (hooks.merge)(doc_id, remote)?;
                received_ops += 1;
            }
        }
        Ok(SyncReport {
            sent_ops,
            received_ops,
            duration_ms: started.elapsed().as_millis() as u64,
        })
    }

    fn watch(&self, callback: Box<dyn Fn(SyncEvent) + Send + Sync>) {
        if let Ok(mut cb) = self.callback.lock() {
            *cb = Some(callback);
        }
    }

    async fn disconnect(&self, conn: &Connection) -> Result<(), crate::Error> {
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
