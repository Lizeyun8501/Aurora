//! SyncGate 接线集成测试（DK-08 §7.3 收官切片）。
//!
//! 验收场景（任务书 §2.3，DST + FakeClock + FakeProvider）：
//! 1. `wifi_only` + Metered → 未传输 + 入队 + 熔断计数不变；
//! 2. Offline → `DeferUntilOnline`，同上；
//! 3. `set_wifi_only(false)` 运行时切换 → Allow（同一 gate 实例）；
//! 4. 恢复重放：gate 放行 → 出队 ack；gate Defer → 原样保留且剩余项不动；
//!    重放不绕过 gate；
//! 5. gate 未注入 → 行为与现状完全一致（Executed / 真错误路径）。

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use aurora_core::traits::sync_target::{
    Connection, DocSet, Endpoint, SyncEvent, SyncProtocol, SyncReport, SyncTarget, UpdatePayload,
};
use aurora_sync::offline_queue::{OfflineQueue, Priority, QueueItem};
use aurora_sync::router::{
    GateOutcome, RouteEntry, RouteTier, RouterPolicy, SharedTarget, SyncRouter,
};
use aurora_sync::sync_gate::{GateDecision, NetworkClass, NetworkStateProvider, SyncGate};

// ── 平台源 fake（三态缓存，模拟 AndroidNetworkState 语义）──

#[derive(Default)]
struct FakeProvider {
    class: AtomicU8,
}

impl FakeProvider {
    fn new(class: NetworkClass) -> Arc<Self> {
        Arc::new(Self {
            class: AtomicU8::new(class as u8),
        })
    }
    fn set(&self, class: NetworkClass) {
        self.class.store(class as u8, Ordering::SeqCst);
    }
}

impl NetworkStateProvider for FakeProvider {
    fn current_class(&self) -> NetworkClass {
        match self.class.load(Ordering::SeqCst) {
            0 => NetworkClass::Unmetered,
            1 => NetworkClass::Metered,
            _ => NetworkClass::Offline,
        }
    }
}

// ── 可成功连接的 mock target（对应 router 测试的 OkTarget 语义）──

#[derive(Default)]
struct OkTarget;

#[async_trait]
impl SyncTarget for OkTarget {
    async fn connect(&mut self, _ep: &Endpoint) -> Result<Connection, aurora_core::Error> {
        Ok(Connection {
            id: "c-ok".into(),
            endpoint: Endpoint {
                url: "ok".into(),
                protocol: SyncProtocol::Iroh,
                auth: None,
            },
        })
    }
    async fn sync(
        &self,
        _conn: &Connection,
        docs: &DocSet,
    ) -> Result<SyncReport, aurora_core::Error> {
        Ok(SyncReport {
            sent_ops: docs.doc_ids.len(),
            received_ops: 0,
            duration_ms: 5,
        })
    }
    async fn send_update(
        &self,
        _c: &Connection,
        _u: &UpdatePayload,
    ) -> Result<(), aurora_core::Error> {
        Ok(())
    }
    async fn recv_update(&self, _c: &Connection, _d: &str) -> Result<Vec<u8>, aurora_core::Error> {
        Ok(Vec::new())
    }
    async fn sync_version(
        &self,
        _c: &Connection,
        _d: &str,
    ) -> Result<Option<u64>, aurora_core::Error> {
        Ok(None)
    }
    fn watch(&self, _cb: Box<dyn Fn(SyncEvent) + Send + Sync>) {}
    async fn disconnect(&self, _conn: &Connection) -> Result<(), aurora_core::Error> {
        Ok(())
    }
}

// ── 测试 fixture：单链路 P2P router ──

fn entries() -> Vec<RouteEntry> {
    vec![RouteEntry {
        target: Arc::new(OkTarget),
        tier: RouteTier::P2p,
        endpoint: Endpoint {
            url: "iroh://a".into(),
            protocol: SyncProtocol::Iroh,
            auth: None,
        },
        privacy: aurora_sync::router::PrivacyLevel::E2eeOnly,
    }]
}

fn gated_router(gate: Arc<SyncGate>) -> SyncRouter {
    SyncRouter::with_gate(
        entries(),
        RouterPolicy::default(),
        Arc::new(aurora_sync::router::FakeClock::new(1000)),
        gate,
    )
}

fn item(doc: &str) -> QueueItem {
    QueueItem::new(doc, vec![1], Priority::Medium).with_idempotency_key(doc)
}

// ═══ 场景 1：wifi_only + Metered → 未传输 + 入队 + 熔断不变 ═══
#[tokio::test]
async fn dk08_wifi_only_metered_defers_without_transfer() {
    let prov = FakeProvider::new(NetworkClass::Metered);
    let gate = Arc::new(SyncGate::new(prov.clone(), true));
    let r = gated_router(gate);

    let mut executed = false;
    let outcome = r
        .route_and_execute(|_d| {
            executed = true;
            async { Ok::<u8, aurora_sync::Error>(0) }
        })
        .await
        .expect("决策本身无错");

    // Defer 决策：execute 未触发、决策可达、非错误
    let GateOutcome::Deferred { gate: gd, .. } = &outcome else {
        panic!("metered + wifi_only 应 Defer: {outcome:?}");
    };
    assert_eq!(*gd, GateDecision::DeferUntilUnmetered);
    assert!(!executed, "未发起传输");

    // 熔断/健康度不变：无失败计数（策略拒绝不是故障）
    let snap = r.health_snapshot();
    assert_eq!(snap[0].1, 0, "consecutive_failures 不变");
    assert!(!snap[0].2, "熔断未打开");

    // 调用方转 OfflineQueue（幂等键保留）
    let q = OfflineQueue::new();
    q.enqueue(item("doc-a")).expect("enqueue");
    assert_eq!(q.len(), 1);
    assert!(q.contains_key("doc-a"));
}

// ═══ 场景 2：Offline → DeferUntilOnline，同上 ═══
#[tokio::test]
async fn dk08_offline_defers_until_online() {
    let prov = FakeProvider::new(NetworkClass::Offline);
    let gate = Arc::new(SyncGate::new(prov.clone(), false)); // 与 wifi_only 无关
    let r = gated_router(gate);

    let outcome = r
        .route_and_execute(|_d| async { Ok::<u8, aurora_sync::Error>(0) })
        .await
        .expect("决策本身无错");
    let GateOutcome::Deferred { gate: gd, .. } = &outcome else {
        panic!("offline 应 Defer: {outcome:?}");
    };
    assert_eq!(*gd, GateDecision::DeferUntilOnline);
}

// ═══ 场景 3：set_wifi_only(false) 运行时切换 → Allow ═══
#[tokio::test]
async fn dk08_runtime_toggle_flips_to_allow() {
    let prov = FakeProvider::new(NetworkClass::Metered);
    let gate = Arc::new(SyncGate::new(prov.clone(), true));
    let r = gated_router(gate.clone());

    // 切换前 Defer
    let outcome = r
        .route_and_execute(|_d| async { Ok::<u8, aurora_sync::Error>(0) })
        .await
        .expect("决策无错");
    assert!(outcome.is_deferred());

    // 同一 gate 实例运行时切换 → Allow（无需重建）
    gate.set_wifi_only(false);
    let mut executed = false;
    let outcome = r
        .route_and_execute(|_d| {
            executed = true;
            async { Ok::<u8, aurora_sync::Error>(7) }
        })
        .await
        .expect("决策无错");
    let (decision, r_val) = outcome.executed().expect("Allow 后应 Executed");
    assert!(executed, "已发起传输");
    assert_eq!(r_val, 7);
    assert_eq!(decision.endpoint_url, "iroh://a");
}

// ═══ 场景 4：恢复重放 —— 放行出队 / Defer 保留 / 不绕过 gate ═══
#[tokio::test]
async fn dk08_replay_releases_on_allow_and_retains_on_defer() {
    let prov = FakeProvider::new(NetworkClass::Metered);
    let gate = Arc::new(SyncGate::new(prov.clone(), true));
    let r = gated_router(gate);

    let q = OfflineQueue::new();
    q.enqueue(item("doc-1")).expect("e1");
    q.enqueue(item("doc-2")).expect("e2");
    q.enqueue(item("doc-3")).expect("e3");
    assert_eq!(q.len(), 3);

    // Defer：当前项原样保留 + 本批剩余项不动
    let summary = r
        .replay_offline_queue(&q, 10, |_i| async { Ok::<(), aurora_sync::Error>(()) })
        .await
        .expect("replay 决策无错");
    assert_eq!(
        (summary.replayed, summary.retained),
        (0, 1),
        "第一项即 Defer"
    );
    assert_eq!(q.len(), 3, "队列无耗损");
    assert!(q.contains_key("doc-1"), "幂等键保留");

    // 放行（Unmetered）：全部重放成功并出队
    prov.set(NetworkClass::Unmetered);
    let summary = r
        .replay_offline_queue(&q, 10, |_i| async { Ok::<(), aurora_sync::Error>(()) })
        .await
        .expect("replay ok");
    assert_eq!((summary.replayed, summary.retained), (3, 0));
    assert_eq!(q.len(), 0, "全部出队");
    assert!(!q.contains_key("doc-1"), "ack 清理幂等索引");
}

// ═══ 场景 5：gate 未注入 —— 行为与现状完全一致 ═══
#[tokio::test]
async fn dk08_no_gate_zero_delta() {
    let r = SyncRouter::with_clock(
        entries(),
        RouterPolicy::default(),
        Arc::new(aurora_sync::router::FakeClock::new(1000)),
    );
    r.attach("iroh://a", SharedTarget::wrap(OkTarget));

    // Executed：sent_ops 与改造前一致
    let outcome = r.sync_via_route(&["doc1".to_string()]).await.expect("ok");
    let (_, report) = outcome.executed().expect("未注入恒 Executed");
    assert_eq!(report.sent_ops, 1);

    // Defer 决策不可能出现
    let outcome2 = r.sync_via_route(&["d".to_string()]).await.unwrap();
    assert!(!outcome2.is_deferred());

    // 真 Err 路径不受影响（无 attach 的 url）
    let r2 = SyncRouter::with_clock(
        entries(),
        RouterPolicy::default(),
        Arc::new(aurora_sync::router::FakeClock::new(1000)),
    );
    let err = r2.sync_via_route(&["d".to_string()]).await.unwrap_err();
    assert!(matches!(err, aurora_sync::Error::Sync(_)));
}
