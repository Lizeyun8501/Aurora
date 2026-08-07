//! 局域网同步 (LAN Sync)
//!
//! 基于 mDNS 自动发现 + LAN IP 直连 QUIC 的局域网同步。
//!
//! # 优先级策略
//! 局域网同步优先于云端同步：当 LAN 内存在可用对端时，
//! 优先通过 LAN 直连传输 (低延迟、零流量)；
//! 仅当 LAN 不可用时回退到云端。参见 [`LanSyncEngine::select_route`]。
//!
//! # 实现说明
//! [`MdnsDiscovery`] 使用内存注册表模拟发现的节点列表。
//! 真实实现使用 `mdns-sd` 或 `zeroconf` crate 在局域网内广播/监听服务。

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

/// LAN 节点。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LanPeer {
    pub peer_id: String,
    pub ip: String,
    pub port: u16,
    pub display_name: String,
    /// RTT (毫秒)，用于优先级排序。
    pub rtt_ms: u32,
}

impl LanPeer {
    pub fn new(peer_id: impl Into<String>, ip: impl Into<String>, port: u16) -> Self {
        Self {
            peer_id: peer_id.into(),
            ip: ip.into(),
            port,
            display_name: String::new(),
            rtt_ms: 0,
        }
    }

    pub fn with_rtt(mut self, rtt_ms: u32) -> Self {
        self.rtt_ms = rtt_ms;
        self
    }

    pub fn with_display_name(mut self, name: impl Into<String>) -> Self {
        self.display_name = name.into();
        self
    }

    /// 返回 `ip:port` 端点字符串。
    pub fn endpoint(&self) -> String {
        format!("{}:{}", self.ip, self.port)
    }
}

/// mDNS 自动发现 (mock 实现)。
///
/// 真实实现使用 `mdns-sd` 或 `zeroconf` crate 在局域网内广播/监听服务。
/// 此处使用内存注册表模拟发现的节点列表，便于测试。
pub struct MdnsDiscovery {
    registry: Arc<RwLock<HashMap<String, LanPeer>>>,
    service_name: String,
}

impl MdnsDiscovery {
    pub fn new(service_name: impl Into<String>) -> Self {
        Self {
            registry: Arc::new(RwLock::new(HashMap::new())),
            service_name: service_name.into(),
        }
    }

    /// 注册本地服务 (模拟 mDNS 广播)。
    pub fn announce(&self, peer: LanPeer) {
        debug!("mdns announce: {} @ {}", peer.peer_id, peer.endpoint());
        self.registry.write().insert(peer.peer_id.clone(), peer);
    }

    /// 取消注册 (模拟 mDNS goodbye)。
    pub fn unannounce(&self, peer_id: &str) {
        self.registry.write().remove(peer_id);
    }

    /// 发现所有可见的 LAN 节点 (不含自己)。
    pub fn discover(&self, self_id: &str) -> Vec<LanPeer> {
        self.registry
            .read()
            .values()
            .filter(|p| p.peer_id != self_id)
            .cloned()
            .collect()
    }

    /// 按 RTT 升序返回发现节点 (优先选择低延迟对端)。
    pub fn discover_sorted(&self, self_id: &str) -> Vec<LanPeer> {
        let mut peers = self.discover(self_id);
        peers.sort_by_key(|p| p.rtt_ms);
        peers
    }

    pub fn service_name(&self) -> &str {
        &self.service_name
    }

    pub fn peer_count(&self) -> usize {
        self.registry.read().len()
    }
}

/// LAN 同步优先级路由结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncRoute {
    /// 使用 LAN 直连。
    Lan(LanPeer),
    /// LAN 不可用，回退到云端。
    Cloud,
}

/// 局域网同步引擎。
pub struct LanSyncEngine {
    discovery: MdnsDiscovery,
    self_id: String,
}

impl LanSyncEngine {
    pub fn new(self_id: impl Into<String>) -> Self {
        let id = self_id.into();
        Self {
            discovery: MdnsDiscovery::new("_aurora._tcp.local."),
            self_id: id,
        }
    }

    pub fn discovery(&self) -> &MdnsDiscovery {
        &self.discovery
    }

    pub fn self_id(&self) -> &str {
        &self.self_id
    }

    /// 注册自身到 mDNS。
    pub fn announce_self(&self, peer: LanPeer) {
        self.discovery.announce(peer);
    }

    /// 选择同步路由：LAN 优先，云端回退。
    pub fn select_route(&self) -> SyncRoute {
        let peers = self.discovery.discover_sorted(&self.self_id);
        if let Some(best) = peers.into_iter().next() {
            info!("lan route selected: {} rtt={}ms", best.peer_id, best.rtt_ms);
            SyncRoute::Lan(best)
        } else {
            info!("no lan peer, fallback to cloud");
            SyncRoute::Cloud
        }
    }

    /// 模拟通过 LAN 直连发送数据。
    pub fn send_via_lan(&self, peer: &LanPeer, data: &[u8]) -> crate::Result<usize> {
        debug!("lan send to {}: {} bytes", peer.endpoint(), data.len());
        Ok(data.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lan_peer_endpoint_and_builders() {
        let peer = LanPeer::new("p1", "192.168.1.10", 11000)
            .with_rtt(5)
            .with_display_name("Alice-Laptop");
        assert_eq!(peer.endpoint(), "192.168.1.10:11000");
        assert_eq!(peer.rtt_ms, 5);
        assert_eq!(peer.display_name, "Alice-Laptop");
    }

    #[test]
    fn test_mdns_discover_excludes_self() {
        let disc = MdnsDiscovery::new("_aurora._tcp.local.");
        disc.announce(LanPeer::new("self", "192.168.1.1", 11000));
        disc.announce(LanPeer::new("peer-a", "192.168.1.2", 11001));
        let found = disc.discover("self");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].peer_id, "peer-a");
    }

    #[test]
    fn test_mdns_discover_sorted_by_rtt() {
        let disc = MdnsDiscovery::new("_aurora._tcp.local.");
        disc.announce(LanPeer::new("slow", "10.0.0.2", 1).with_rtt(100));
        disc.announce(LanPeer::new("fast", "10.0.0.3", 1).with_rtt(2));
        disc.announce(LanPeer::new("mid", "10.0.0.4", 1).with_rtt(50));
        let sorted = disc.discover_sorted("self");
        assert_eq!(sorted.len(), 3);
        assert_eq!(sorted[0].peer_id, "fast");
        assert_eq!(sorted[1].peer_id, "mid");
        assert_eq!(sorted[2].peer_id, "slow");
    }

    #[test]
    fn test_lan_route_selection_prefers_lan() {
        let engine = LanSyncEngine::new("self");
        engine.announce_self(LanPeer::new("self", "192.168.1.1", 11000));
        // 注册一个低延迟对端
        engine
            .discovery()
            .announce(LanPeer::new("peer", "192.168.1.2", 11001).with_rtt(3));
        let route = engine.select_route();
        match route {
            SyncRoute::Lan(peer) => assert_eq!(peer.peer_id, "peer"),
            SyncRoute::Cloud => panic!("should select LAN"),
        }
    }

    #[test]
    fn test_lan_route_fallback_to_cloud() {
        let engine = LanSyncEngine::new("self");
        // 仅注册自己，无对端
        engine.announce_self(LanPeer::new("self", "192.168.1.1", 11000));
        let route = engine.select_route();
        assert_eq!(route, SyncRoute::Cloud);
    }

    #[test]
    fn test_lan_announce_unannounce() {
        let disc = MdnsDiscovery::new("_aurora._tcp.local.");
        disc.announce(LanPeer::new("p1", "10.0.0.1", 1));
        assert_eq!(disc.peer_count(), 1);
        disc.unannounce("p1");
        assert_eq!(disc.peer_count(), 0);
    }

    #[test]
    fn test_lan_send_via_lan_returns_size() {
        let engine = LanSyncEngine::new("self");
        let peer = LanPeer::new("peer", "192.168.1.2", 11001);
        let data = vec![0u8; 1234];
        let sent = engine.send_via_lan(&peer, &data).expect("send");
        assert_eq!(sent, 1234);
    }

    #[test]
    fn test_lan_service_name() {
        let engine = LanSyncEngine::new("self");
        assert!(engine.discovery().service_name().contains("aurora"));
    }
}
