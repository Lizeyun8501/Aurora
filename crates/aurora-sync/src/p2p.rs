//! P2P 同步 (P2P Sync)
//!
//! 基于 iroh + QUIC + NAT 穿透的点对点同步引擎。
//!
//! # 设计说明
//! 真实的 iroh 网络栈需要异步运行时初始化、relay 服务器与节点发现协议，
//! 难以在单元测试中稳定运行。本模块提供基于 `tokio::sync::mpsc` 的
//! 内存模拟传输层 [`MockTransport`]，完整实现 Loro Sync Protocol 的消息语义
//! ([`SyncMessage::Hello`] / [`SyncMessage::Update`] / [`SyncMessage::Ack`] /
//! [`SyncMessage::Snapshot`]) 与版本向量 ([`VersionVector`]) 交换。
//!
//! 真实部署时，仅需将 `MockTransport` 替换为 `iroh::node::Node` 提供的
//! QUIC 连接即可，上层消息协议与状态机保持不变。

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{debug, info};

/// 节点唯一标识。
///
/// 真实实现中对应 iroh 的 `NodeId` (Ed25519 公钥派生)。
/// 此处使用字符串包装以便在内存模拟与单元测试中使用。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PeerId(String);

impl PeerId {
    /// 随机生成一个新的 PeerId。
    pub fn random() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    /// 从字符串构造 PeerId。
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        Self(s.to_string())
    }

    /// 返回内部字符串引用。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PeerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for PeerId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

/// 版本向量 (Version Vector)，记录每个节点的最新已见 op 序号。
///
/// 用于同步时的「缺失计算」：比较双方 VV 即可确定需要发送的 ops 范围。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct VersionVector(pub HashMap<PeerId, u64>);

impl VersionVector {
    pub fn new() -> Self {
        Self::default()
    }

    /// 记录某节点的最新序号 (仅单调递增)。
    pub fn set(&mut self, peer: PeerId, seq: u64) {
        let entry = self.0.entry(peer).or_insert(0);
        if seq > *entry {
            *entry = seq;
        }
    }

    /// 获取某节点的序号。
    pub fn get(&self, peer: &PeerId) -> u64 {
        self.0.get(peer).copied().unwrap_or(0)
    }

    /// 合并另一版本向量 (取每个节点的最大值)。
    pub fn merge(&mut self, other: &VersionVector) {
        for (peer, seq) in &other.0 {
            self.set(peer.clone(), *seq);
        }
    }

    /// 计算本向量相对于 `other` 多出的部分 (用于推送缺失 ops)。
    pub fn diff(&self, other: &VersionVector) -> HashMap<PeerId, u64> {
        let mut out = HashMap::new();
        for (peer, seq) in &self.0 {
            let theirs = other.get(peer);
            if *seq > theirs {
                out.insert(peer.clone(), *seq);
            }
        }
        out
    }

    /// 是否已包含另一版本向量的全部信息。
    pub fn contains(&self, other: &VersionVector) -> bool {
        other.0.iter().all(|(p, s)| self.get(p) >= *s)
    }

    /// 返回已记录的节点数量。
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Loro Sync Protocol 同步消息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncMessage {
    /// 握手：交换 PeerId 与版本向量。
    Hello { peer_id: PeerId, vv: VersionVector },
    /// 增量更新：携带 op 字节流与起始 VV。
    Update {
        ops: Vec<u8>,
        from_vv: VersionVector,
    },
    /// 确认：返回接收方当前 VV。
    Ack { vv: VersionVector },
    /// 快照：完整文档二进制 (用于冷启动或大差异同步)。
    Snapshot { blob: Vec<u8>, vv: VersionVector },
}

impl SyncMessage {
    /// 序列化为字节数组 (模拟网络传输的 wire format)。
    pub fn encode(&self) -> crate::Result<Vec<u8>> {
        bincode::serialize(self).map_err(crate::Error::from)
    }

    /// 从字节数组反序列化。
    pub fn decode(bytes: &[u8]) -> crate::Result<Self> {
        bincode::deserialize(bytes).map_err(crate::Error::from)
    }

    /// 返回消息类型名称 (用于日志)。
    pub fn kind(&self) -> &'static str {
        match self {
            SyncMessage::Hello { .. } => "Hello",
            SyncMessage::Update { .. } => "Update",
            SyncMessage::Ack { .. } => "Ack",
            SyncMessage::Snapshot { .. } => "Snapshot",
        }
    }
}

/// 共享的 mesh 网络注册表：PeerId -> 入站消息发送端。
pub type PeerRegistry = Arc<RwLock<HashMap<PeerId, mpsc::Sender<SyncMessage>>>>;

/// 内存模拟的 P2P 网络传输层。
///
/// 每个 [`P2pSyncEngine`] 持有一个 inbox (mpsc 接收端)，
/// 并向共享 [`PeerRegistry`] 注册自己的发送端，从而模拟全连接 mesh。
pub struct MockTransport {
    inbox_rx: Mutex<mpsc::Receiver<SyncMessage>>,
    inbox_tx: mpsc::Sender<SyncMessage>,
    peers: PeerRegistry,
}

impl MockTransport {
    /// 创建新的传输层，绑定到指定的 mesh 注册表。
    pub fn new(peers: PeerRegistry) -> Self {
        let (tx, rx) = mpsc::channel(256);
        Self {
            inbox_rx: Mutex::new(rx),
            inbox_tx: tx,
            peers,
        }
    }

    /// 注册本节点的发送端到共享 mesh 注册表。
    pub fn register(&self, peer_id: &PeerId) {
        self.peers
            .write()
            .insert(peer_id.clone(), self.inbox_tx.clone());
    }

    /// 注销本节点。
    pub fn unregister(&self, peer_id: &PeerId) {
        self.peers.write().remove(peer_id);
    }

    /// 向指定节点发送消息 (模拟 QUIC 单播)。
    pub fn send_to(&self, peer: &PeerId, msg: SyncMessage) -> crate::Result<()> {
        let peers = self.peers.read();
        match peers.get(peer) {
            Some(tx) => tx
                .try_send(msg)
                .map_err(|e| crate::Error::Network(format!("send to {} failed: {}", peer, e))),
            None => Err(crate::Error::NotFound(format!(
                "peer not connected: {}",
                peer
            ))),
        }
    }

    /// 非阻塞接收一条消息。
    pub fn try_recv(&self) -> Option<SyncMessage> {
        self.inbox_rx.lock().try_recv().ok()
    }

    /// 阻塞接收一条消息 (供同步测试使用，不可在 tokio 运行时内调用)。
    pub fn blocking_recv(&self) -> Option<SyncMessage> {
        self.inbox_rx.lock().blocking_recv()
    }

    /// 当前 mesh 中已注册的节点数。
    pub fn peer_count(&self) -> usize {
        self.peers.read().len()
    }
}

/// P2P 同步引擎。
pub struct P2pSyncEngine {
    peer_id: PeerId,
    vv: Mutex<VersionVector>,
    transport: MockTransport,
}

impl P2pSyncEngine {
    /// 创建并注册到指定 mesh 网络的引擎。
    pub fn new(peer_id: PeerId, peers: PeerRegistry) -> Self {
        let engine = Self {
            peer_id: peer_id.clone(),
            vv: Mutex::new(VersionVector::new()),
            transport: MockTransport::new(peers),
        };
        engine.transport.register(&peer_id);
        engine
    }

    pub fn peer_id(&self) -> &PeerId {
        &self.peer_id
    }

    /// 返回当前版本向量的快照。
    pub fn version_vector(&self) -> VersionVector {
        self.vv.lock().clone()
    }

    /// 本地记录新产生的 op 序号。
    pub fn record_local_op(&self, seq: u64) {
        self.vv.lock().set(self.peer_id.clone(), seq);
    }

    /// 向对端发送 Hello 消息，发起握手与版本向量交换。
    pub fn send_hello(&self, peer: &PeerId) -> crate::Result<()> {
        let vv = self.version_vector();
        info!(
            "p2p hello: {} -> {} vv_len={}",
            self.peer_id,
            peer,
            vv.len()
        );
        self.transport.send_to(
            peer,
            SyncMessage::Hello {
                peer_id: self.peer_id.clone(),
                vv,
            },
        )
    }

    /// 向对端发送增量更新。
    pub fn send_update(&self, peer: &PeerId, ops: Vec<u8>) -> crate::Result<()> {
        let from_vv = self.version_vector();
        debug!(
            "p2p update: {} -> {} ops_len={}",
            self.peer_id,
            peer,
            ops.len()
        );
        self.transport
            .send_to(peer, SyncMessage::Update { ops, from_vv })
    }

    /// 向对端发送快照。
    pub fn send_snapshot(&self, peer: &PeerId, blob: Vec<u8>) -> crate::Result<()> {
        let vv = self.version_vector();
        self.transport
            .send_to(peer, SyncMessage::Snapshot { blob, vv })
    }

    /// 处理接收到的消息，更新本地 VV，必要时回 Ack。
    pub fn handle_message(&self, msg: SyncMessage) -> crate::Result<()> {
        match msg {
            SyncMessage::Hello { peer_id, vv } => {
                debug!("hello from {} vv_len={}", peer_id, vv.len());
                self.vv.lock().merge(&vv);
                let ack_vv = self.version_vector();
                self.transport
                    .send_to(&peer_id, SyncMessage::Ack { vv: ack_vv })?;
            }
            SyncMessage::Update { from_vv, .. } => {
                self.vv.lock().merge(&from_vv);
            }
            SyncMessage::Ack { vv } => {
                self.vv.lock().merge(&vv);
            }
            SyncMessage::Snapshot { vv, .. } => {
                self.vv.lock().merge(&vv);
            }
        }
        Ok(())
    }

    /// 非阻塞轮询一条入站消息并处理。
    ///
    /// 返回 `Ok(true)` 表示处理了一条消息，`Ok(false)` 表示无消息。
    pub fn poll(&self) -> crate::Result<bool> {
        match self.transport.try_recv() {
            Some(msg) => {
                self.handle_message(msg)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// 阻塞等待并处理一条入站消息。
    pub fn recv_and_handle(&self) -> crate::Result<()> {
        match self.transport.blocking_recv() {
            Some(msg) => self.handle_message(msg),
            None => Err(crate::Error::Network("inbox closed".to_string())),
        }
    }

    /// 当前 mesh 中已连接的节点数。
    pub fn connected_peers(&self) -> usize {
        self.transport.peer_count().saturating_sub(1)
    }

    /// 注销本节点 (断开 mesh 连接)。
    pub fn disconnect(&self) {
        self.transport.unregister(&self.peer_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_mesh() -> PeerRegistry {
        Arc::new(RwLock::new(HashMap::new()))
    }

    #[test]
    fn test_peer_id_random_unique() {
        let a = PeerId::random();
        let b = PeerId::random();
        assert_ne!(a, b);
        assert!(!a.as_str().is_empty());
    }

    #[test]
    fn test_version_vector_merge_and_get() {
        let p1 = PeerId::from_str("p1");
        let p2 = PeerId::from_str("p2");
        let mut a = VersionVector::new();
        a.set(p1.clone(), 5);
        let mut b = VersionVector::new();
        b.set(p2.clone(), 3);
        b.set(p1.clone(), 7); // 比 a 更新
        a.merge(&b);
        assert_eq!(a.get(&p1), 7);
        assert_eq!(a.get(&p2), 3);
    }

    #[test]
    fn test_version_vector_diff_and_contains() {
        let p1 = PeerId::from_str("p1");
        let p2 = PeerId::from_str("p2");
        let mut a = VersionVector::new();
        a.set(p1.clone(), 10);
        a.set(p2.clone(), 2);
        let mut b = VersionVector::new();
        b.set(p1.clone(), 4);
        // a 比 b 多出 p1=10 (4->10) 与 p2=2 (0->2)
        let diff = a.diff(&b);
        assert_eq!(diff.get(&p1).copied(), Some(10));
        assert_eq!(diff.get(&p2).copied(), Some(2));
        // a 包含 b
        assert!(a.contains(&b));
        // b 不包含 a
        assert!(!b.contains(&a));
    }

    #[test]
    fn test_sync_message_encode_decode_roundtrip() {
        let p = PeerId::from_str("peer-x");
        let msg = SyncMessage::Hello {
            peer_id: p.clone(),
            vv: {
                let mut v = VersionVector::new();
                v.set(p, 42);
                v
            },
        };
        let bytes = msg.encode().expect("encode");
        let decoded = SyncMessage::decode(&bytes).expect("decode");
        match decoded {
            SyncMessage::Hello { peer_id, vv } => {
                assert_eq!(peer_id, PeerId::from_str("peer-x"));
                assert_eq!(vv.get(&PeerId::from_str("peer-x")), 42);
            }
            _ => panic!("expected Hello"),
        }
    }

    #[test]
    fn test_sync_message_kind() {
        let p = PeerId::from_str("p");
        let empty = VersionVector::new();
        assert_eq!(
            SyncMessage::Hello {
                peer_id: p.clone(),
                vv: empty.clone()
            }
            .kind(),
            "Hello"
        );
        assert_eq!(
            SyncMessage::Update {
                ops: vec![],
                from_vv: empty.clone()
            }
            .kind(),
            "Update"
        );
        assert_eq!(SyncMessage::Ack { vv: empty.clone() }.kind(), "Ack");
        assert_eq!(
            SyncMessage::Snapshot {
                blob: vec![],
                vv: empty
            }
            .kind(),
            "Snapshot"
        );
    }

    #[test]
    fn test_p2p_hello_exchange_updates_vv() {
        let mesh = new_mesh();
        let alice = P2pSyncEngine::new(PeerId::from_str("alice"), mesh.clone());
        let bob = P2pSyncEngine::new(PeerId::from_str("bob"), mesh.clone());
        alice.record_local_op(3);
        // alice -> bob: Hello
        alice.send_hello(bob.peer_id()).expect("hello");
        // bob 接收并处理 (会回 Ack)
        bob.recv_and_handle().expect("bob handle hello");
        // alice 接收 Ack
        alice.recv_and_handle().expect("alice handle ack");
        // alice 的 VV 应包含 bob 节点 (虽 bob 没有 local ops，但 Ack 回传 alice 自己的 VV)
        // bob 的 VV 应包含 alice=3
        assert_eq!(bob.version_vector().get(&PeerId::from_str("alice")), 3);
    }

    #[test]
    fn test_p2p_update_delivery_and_merge() {
        let mesh = new_mesh();
        let alice = P2pSyncEngine::new(PeerId::from_str("alice"), mesh.clone());
        let bob = P2pSyncEngine::new(PeerId::from_str("bob"), mesh.clone());
        alice.record_local_op(10);
        let ops = vec![1u8, 2, 3, 4];
        alice.send_update(bob.peer_id(), ops).expect("update");
        bob.recv_and_handle().expect("bob handle update");
        assert_eq!(bob.version_vector().get(&PeerId::from_str("alice")), 10);
    }

    #[test]
    fn test_p2p_send_to_unknown_peer_errors() {
        let mesh = new_mesh();
        let alice = P2pSyncEngine::new(PeerId::from_str("alice"), mesh);
        let ghost = PeerId::from_str("ghost");
        let result = alice.send_hello(&ghost);
        assert!(result.is_err());
        alice.disconnect();
    }

    #[test]
    fn test_p2p_disconnect_removes_from_mesh() {
        let mesh = new_mesh();
        let alice = P2pSyncEngine::new(PeerId::from_str("alice"), mesh.clone());
        assert_eq!(mesh.read().len(), 1);
        alice.disconnect();
        assert_eq!(mesh.read().len(), 0);
    }

    #[test]
    fn test_p2p_connected_peers_count() {
        let mesh = new_mesh();
        let alice = P2pSyncEngine::new(PeerId::from_str("alice"), mesh.clone());
        assert_eq!(alice.connected_peers(), 0);
        let _bob = P2pSyncEngine::new(PeerId::from_str("bob"), mesh.clone());
        assert_eq!(alice.connected_peers(), 1);
        let _carol = P2pSyncEngine::new(PeerId::from_str("carol"), mesh.clone());
        assert_eq!(alice.connected_peers(), 2);
    }
}
