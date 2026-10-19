//! DK-2.8v2: iroh 直传分享 e2e——本地 relay 双节点对拍（DK-46 先例基建复用）。
//!
//! 1. 直传 ack 语义：B send_share → A 收件循环落 sink → B 收到 Ok（ack）
//! 2. ALPN 分流不冒收：A 收件循环只处理 SHARE_ALPN——B 发起的 sync 协议
//!    连接被丢弃（A 无 sync accept 面——B sync_with_peer 显式失败，不静默
//!    半通）
//!
//! 测试无外部依赖：进程内 relay（self-signed TLS + QUIC addr）。

#![cfg(feature = "iroh-transport")]

use std::sync::Arc;
use std::time::Duration;

use aurora_sync::iroh_transport::{
    run_share_accept_loop, IrohTransport, ShareInboxSink, SHARE_ALPN,
};
use aurora_sync::PeerId;
use rustls_pki_types::CertificateDer;
use tokio::time::timeout;

fn init_log() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_test_writer()
        .try_init();
}

/// 测试整体护栏（DK-46 同款：150s 硬超时防 CI 挂死）。
macro_rules! guarded {
    ($body:expr) => {
        tokio::time::timeout(std::time::Duration::from_secs(150), $body)
            .await
            .expect("测试整体护栏超时（150s）——内部存在死等点，CI 挂死保护触发")
    };
}

/// 本地 relay server（进程内——测试结束 shutdown）。DK-46 spawn_local_relay
/// 同款（self-signed TLS + QUIC addr——官方 patchbay 范式）。
struct LocalRelay {
    _server: iroh_relay::server::Server,
    map: iroh::RelayMap,
    certs: Vec<CertificateDer<'static>>,
}

async fn spawn_local_relay() -> LocalRelay {
    let bind_ip: std::net::IpAddr = "127.0.0.1".parse().unwrap();
    let (certs, server_config) = iroh_relay::server::testing::self_signed_tls_certs_and_config();
    let tls = iroh_relay::server::TlsConfig::new(
        (bind_ip, 0),
        iroh_relay::server::CertConfig::Manual { server_config },
    );
    let mut relay_cfg = iroh_relay::server::RelayConfig::new((bind_ip, 0));
    relay_cfg.tls = Some(tls);
    let mut config = iroh_relay::server::ServerConfig::default();
    config.relay = Some(relay_cfg);
    config.quic = Some(iroh_relay::server::QuicConfig::new((bind_ip, 0)));
    let server = iroh_relay::server::Server::spawn(config)
        .await
        .expect("local relay spawn");
    let url = server.https_url().expect("local relay https url");
    let quic_port = server.quic_addr().expect("local relay quic addr").port();
    let map: iroh::RelayMap = iroh::RelayConfig::new(
        url.clone(),
        Some(iroh_relay::RelayQuicConfig::new(quic_port)),
    )
    .into();
    LocalRelay {
        _server: server,
        map,
        certs,
    }
}

/// relay 端点（自签证书显式信任根——DK-46 CI 修复同款；同机直连不经 relay）。
async fn spawn_relay_node(tag: &str, relay: &LocalRelay) -> Arc<IrohTransport> {
    let peer_id = PeerId::from_str(tag);
    let t =
        IrohTransport::new_with_relay_map(peer_id, relay.map.clone(), Some(relay.certs.clone()))
            .await
            .expect("relay node bind");
    Arc::new(t)
}

/// DoD 1: 直传 ack 语义——send_share 等对端落箱才 Ok，sink 收到原包。
#[tokio::test]
async fn dk28v2_share_direct_delivery_with_ack() {
    init_log();
    guarded!(async {
        let relay = spawn_local_relay().await;
        let a = spawn_relay_node("share-a", &relay).await;
        let b = spawn_relay_node("share-b", &relay).await;
        a.wait_online().await;
        b.wait_online().await;

        // A 侧：收件循环 + sink 收集（mpsc 桥 sync 回调→async 断言）
        let (tx, mut rx) = tokio::sync::mpsc::channel::<(Vec<u8>, String)>(8);
        let sink: ShareInboxSink = Arc::new(move |pack, peer| {
            let tx = tx.clone();
            tokio::spawn(async move {
                let _ = tx.send((pack, peer)).await;
            });
        });
        let (_shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        tokio::spawn(run_share_accept_loop(a.clone(), shutdown_rx, sink));

        // B 侧：直传（对端地址含直连候选——同机秒连）
        let pack = r##"{"title":"直传笔记","content":"#DK-2.8v2","createdAt":""}"##.as_bytes();
        let report = timeout(Duration::from_secs(20), b.send_share(a.addr(), pack))
            .await
            .expect("send_share 护栏超时")
            .expect("send_share 须 Ok（ack 语义——对端落箱才返回）");
        assert!(report.success);
        assert_eq!(report.sent_bytes, pack.len());

        // A 侧断言：sink 收到原包（字节级）
        let (got, peer) = timeout(Duration::from_secs(10), rx.recv())
            .await
            .expect("sink 收包护栏超时")
            .expect("sink channel 关闭");
        assert_eq!(got, pack, "sink 须收到原包（字节级）");
        assert_eq!(peer, "paired-device");
    });
}

/// DoD 2: ALPN 分流不冒收——sync 协议连接被 share loop 丢弃（显式失败）。
#[tokio::test]
async fn dk28v2_share_loop_drops_sync_connections() {
    init_log();
    guarded!(async {
        let relay = spawn_local_relay().await;
        let a = spawn_relay_node("share-drop-a", &relay).await;
        let b = spawn_relay_node("share-drop-b", &relay).await;
        a.wait_online().await;
        b.wait_online().await;

        let (_tx, mut rx) = tokio::sync::mpsc::channel::<(Vec<u8>, String)>(8);
        let tx_for_sink = _tx.clone();
        let sink: ShareInboxSink = Arc::new(move |pack, peer| {
            let tx = tx_for_sink.clone();
            tokio::spawn(async move {
                let _ = tx.send((pack, peer)).await;
            });
        });
        let (_shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        tokio::spawn(run_share_accept_loop(a.clone(), shutdown_rx, sink));

        // B 发起 sync 协议连接（AURORA_ALPN）——A 侧 share loop 应丢弃，
        // B 得到显式错误（对端 accept_bi 超时/连接关闭）而非静默成功
        let doc = loro::LoroDoc::new();
        let result = timeout(Duration::from_secs(25), b.sync_with_peer(a.addr(), &doc)).await;
        match result {
            Ok(Err(_)) => {} // 显式失败 ✓
            Err(_) => {}     // 护栏超时 ✓（A 从不 accept_bi——同效）
            Ok(Ok(report)) => panic!("sync 连接不该在 share loop 成功: {report:?}"),
        }
        // sink 须为空（无冒收）
        assert!(
            rx.try_recv().is_err(),
            "share loop 不得收下 sync 协议连接的包"
        );
    });
}

/// 常量自检：SHARE_ALPN 与同步面隔离（守两协议不串面的设计承诺）。
#[test]
fn dk28v2_share_alpn_distinct_from_sync() {
    assert!(
        !std::str::from_utf8(SHARE_ALPN).unwrap().contains("note"),
        "share ALPN 须独立命名空间"
    );
}
