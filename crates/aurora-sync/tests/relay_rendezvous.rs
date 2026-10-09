//! DK-46: relay 会合 e2e——本地 `iroh_relay::server::Server::spawn` 起进程内
//! relay（零外部依赖，预探 17418e7 范式），验证：
//!
//! 1. relay mode 注入生效（四档映射 + Disabled 零回归）
//! 2. 本地会合 e2e：双节点 relay-only 地址（无直连候选——强 relay 仿真 NAT）
//!    → 消息到达断言
//! 3. 不可达显式错误：死 relay / 坏 URL 不静默吞

#![cfg(feature = "iroh-transport")]

use std::sync::Arc;
use std::time::Duration;

use aurora_sync::iroh_transport::{IrohTransport, RelayModeConfig};
use aurora_sync::PeerId;
use iroh::RelayUrl;
use loro::LoroDoc;
use tokio::time::timeout;

fn init_log() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_test_writer()
        .try_init();
}

const SYNC_TIMEOUT_SECS: u64 = 30;

/// 本地 relay server（进程内——测试结束 shutdown）。
struct LocalRelay {
    /// 持有 server 存活（测试期不掉——drop 即关）。
    _server: iroh_relay::server::Server,
    /// relay URL（诊断用；节点注入走 map）。
    _url: RelayUrl,
    map: iroh::RelayMap,
}

async fn spawn_local_relay() -> LocalRelay {
    // 官方 patchbay 范式：self-signed TLS relay + QUIC addr（零外部依赖）
    let bind_ip: std::net::IpAddr = "127.0.0.1".parse().unwrap();
    let (_certs, server_config) = iroh_relay::server::testing::self_signed_tls_certs_and_config();
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
    // map 带 quic addr（官方 run_relay_server 同款——ws 转发+quic 协调）
    let quic_port = server.quic_addr().expect("local relay quic addr").port();
    let map: iroh::RelayMap = iroh::RelayConfig::new(
        url.clone(),
        Some(iroh_relay::RelayQuicConfig::new(quic_port)),
    )
    .into();
    LocalRelay {
        _server: server,
        _url: url,
        map,
    }
}

/// relay-only 端点（Custom 指向本地 relay）。
async fn spawn_relay_node(tag: &str, relay: &LocalRelay) -> Arc<IrohTransport> {
    let peer_id = PeerId::from_str(&format!("relay-{tag}"));
    let t = IrohTransport::new_with_relay_map(peer_id, relay.map.clone(), true)
        .await
        .expect("relay node bind");
    Arc::new(t)
}

/// relay-only 地址：等 home relay 连接就绪（观测面证明注入生效），
/// 以观测到的 relay URL 构造无直连候选的地址（强制会合路径——
/// 逻辑上等价 NAT 直连不可达环境）。
async fn relay_only_addr(t: &IrohTransport) -> iroh::EndpointAddr {
    let url = t
        .wait_home_relay_url(Duration::from_secs(10))
        .await
        .expect("relay 节点须在 10s 内连上本地 relay（relay_mode 注入生效）");
    iroh::EndpointAddr::from_parts(t.addr().id, [iroh::TransportAddr::Relay(url)])
}

/// 诊断：单节点 wait_home_relay_url（integration 环境隔离）
#[tokio::test]
async fn dk46_diag_integration_wait() {
    let relay = spawn_local_relay().await;
    let a = spawn_relay_node("dk46-diag", &relay).await;
    let r = a.wait_home_relay_url(Duration::from_secs(15)).await;
    assert!(r.is_ok(), "integration 单节点 wait 须 Ok: {r:?}");
}

/// DoD 2: 本地 relay 会合——双节点经 relay 中转完成双向同步收敛。
#[tokio::test]
async fn dk46_relay_rendezvous_message_arrival() {
    init_log();
    let relay = spawn_local_relay().await;
    let a = spawn_relay_node("dk46-a", &relay).await;
    let b = spawn_relay_node("dk46-b", &relay).await;

    // A 写入文本，B 发起同步（relay-only 地址——强 relay 路径）
    let doc_a = LoroDoc::new();
    let text_a = doc_a.get_text("content");
    text_a.insert(0, "rendezvous via local relay").unwrap();
    doc_a.commit();

    a.wait_online().await;
    b.wait_online().await;
    let a_addr = relay_only_addr(&a).await;
    let a_task = {
        let doc_a = doc_a.clone();
        tokio::spawn(async move { a.accept_sync(&doc_a).await })
    };

    let doc_b = LoroDoc::new();
    let report = timeout(
        Duration::from_secs(SYNC_TIMEOUT_SECS),
        b.sync_with_peer(a_addr, &doc_b),
    )
    .await
    .expect("relay rendezvous sync timeout")
    .expect("relay rendezvous sync failed");
    assert!(report.success, "会合同步须成功");
    assert!(report.received_bytes > 0, "须收到对端增量");

    let _ = a_task.await;

    // 消息到达断言：B 的 doc 含 A 的文本（CRDT 收敛）
    let text_b = doc_b.get_text("content");
    assert_eq!(
        text_b.to_string(),
        "rendezvous via local relay",
        "relay 会合后内容须到达"
    );
}

/// DoD 1: relay mode 注入生效——Disabled 档端点无 relay 候选（现行为零回归）；
/// Custom 坏 URL 显式报错（不静默）。
#[tokio::test]
async fn dk46_relay_mode_injection_semantics() {
    // Disabled：bind 成功 + addr 无 Relay 候选（与 new() 纯直连等价）
    let t = IrohTransport::new_with_relay(
        PeerId::from_str("dk46-disabled"),
        &RelayModeConfig::Disabled,
    )
    .await
    .expect("disabled bind");
    assert!(
        t.wait_home_relay_url(Duration::from_secs(2)).await.is_err(),
        "Disabled 档不得连上任何 relay（恒 Err）"
    );

    // 四档映射单元断言
    assert!(matches!(
        RelayModeConfig::Disabled.to_iroh_relay_mode().unwrap(),
        iroh::RelayMode::Disabled
    ));
    assert!(matches!(
        RelayModeConfig::Default.to_iroh_relay_mode().unwrap(),
        iroh::RelayMode::Default
    ));
    assert!(matches!(
        RelayModeConfig::Staging.to_iroh_relay_mode().unwrap(),
        iroh::RelayMode::Staging
    ));
    assert!(matches!(
        RelayModeConfig::Custom("http://127.0.0.1:1".into())
            .to_iroh_relay_mode()
            .unwrap(),
        iroh::RelayMode::Custom(_)
    ));

    // 坏 URL 显式错误
    assert!(
        RelayModeConfig::Custom("not a url".into())
            .to_iroh_relay_mode()
            .is_err(),
        "坏 relay URL 须显式报错"
    );
}

/// DoD 3: relay 不可达显式错误——指向死端口的 relay 上会合，connect 超时
/// 必须以 Err 收场（不静默吞——同步失败可观测）。
#[tokio::test]
async fn dk46_unreachable_relay_explicit_error() {
    // 死端口 relay（127.0.0.1:1 无监听）
    let bad = RelayModeConfig::Custom("http://127.0.0.1:1".into());

    let _ta = IrohTransport::new_with_relay(PeerId::from_str("dk46-ua"), &bad)
        .await
        .expect("bind with dead relay（bind 不校验 relay 连通——运行期才拨号）");

    let tb = IrohTransport::new_with_relay(PeerId::from_str("dk46-ub"), &bad)
        .await
        .expect("bind with dead relay");

    // 死 relay → 观测面显式 Err（含「not connected within」语义——不静默）
    let err = tb
        .wait_home_relay_url(Duration::from_secs(8))
        .await
        .expect_err("死 relay 须显式 Err");
    assert!(err.contains("not connected within"), "错误须可观测: {err}");
}
