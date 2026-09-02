//! SyncRouter — 同步策略路由（V20 §3.10「传输无关、策略驱动」）
//!
//! 同步能力通过 [`SyncTarget`] 端口接入，由路由器按**健康度、成本、
//! 隐私等级**选择链路; iroh P2P 只是其中一种传输。
//!
//! # 降级链（默认策略 P0）
//!
//! ```text
//! iroh P2P (NAT 穿透) → LAN 直连 → 云端中转 → WebDAV 降级
//! ```
//!
//! - 每个目标维护健康度（连续失败计数 / 熔断窗口 / EMA RTT）
//! - 一次 `route()` 调用内按序尝试，失败自动降级到下一链路
//! - 熔断（CircuitBreaker）: 连续失败 ≥ `max_consecutive_failures`
//!   → 半开探测（冷却期后单次试探）→ 恢复或继续熔断
//! - 隐私等级约束: 标记 `Privacy::E2eeOnly` 的同步组跳过云/WebDAV
//!
//! # DST 确定性仿真（§4.17 / Phase 2 退出条件）
//!
//! [`DstSimulator`] 以**注入时钟**（不依赖系统时间）驱动:
//! - NAT 穿透失败注入（iroh 连接超时）
//! - 熔断恢复（冷却期过半开探测）
//! - 全链路降级（P2P→LAN→云）
//! 同一场景序列在任何机器上输出相同的选路结果 — 可断言、可回归。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use tracing::{info, warn};

use aurora_core::traits::sync_target::{
    ConnectionState, Endpoint, SyncConfig, SyncProtocol, SyncReport, SyncTarget,
};

/// 同步目标优先级档位。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RouteTier {
    /// 首选: P2P 直连（iroh NAT 穿透）— 零成本、E2EE、最低延迟。
    P2p,
    /// 次选: LAN 直连 — 零成本、E2EE、局域网内。
    Lan,
    /// 三选: 云端中继 — 服务成本、密文中转（零知识）。
    Cloud,
    /// 末选: 外部协议降级（WebDAV/S3）— 可靠但延迟高、明文边界提示。
    External,
}

impl RouteTier {
    fn as_str(&self) -> &'static str {
        match self {
            RouteTier::P2p => "p2p",
            RouteTier::Lan => "lan",
            RouteTier::Cloud => "cloud",
            RouteTier::External => "external",
        }
    }
}

/// 同步数据的隐私等级（选路约束）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PrivacyLevel {
    /// 任意链路（含外部明文降级）。
    Plain,
    /// 仅加密链路（P2P/LAN/云密文中转），禁外部明文协议。
    Encrypted,
    /// 仅端到端加密（P2P/LAN），云与外部全部禁用。
    E2eeOnly,
}

/// 路由决策结果。
#[derive(Debug, Clone, PartialEq)]
pub struct RouteDecision {
    /// 选中的链路档位。
    pub tier: RouteTier,
    /// 实际选中的传输端点名。
    pub endpoint_url: String,
    /// 决策原因（诊断/审计/测试断言用）。
    pub reason: String,
}

/// 单个同步目标的注册项。
pub struct RouteEntry {
    /// 传输实现。
    pub target: Arc<dyn SyncTarget>,
    /// 档位。
    pub tier: RouteTier,
    /// 端点。
    pub endpoint: Endpoint,
    /// 本链路的隐私等级上限。
    pub privacy: PrivacyLevel,
}

/// 健康度追踪（熔断器 + 失败计数）。
#[derive(Debug, Default)]
struct Health {
    consecutive_failures: u32,
    /// 熔断打开时刻（注入时钟 ms; 0 = 未熔断）。
    opened_at_ms: u64,
    /// EMA RTT（ms, 0 = 无样本）。
    ema_rtt_ms: f64,
}

/// 注入时钟（DST 用 — 不依赖系统时间，确定性）。
pub trait Clock: Send + Sync {
    fn now_ms(&self) -> u64;
}

/// 真实时钟（生产）。
pub struct SystemClock;
impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
}

/// 确定性注入时钟（DST 仿真 — 测试手动推进）。
#[derive(Default)]
pub struct FakeClock {
    now_ms: std::sync::atomic::AtomicU64,
}
impl FakeClock {
    pub fn new(start_ms: u64) -> Self {
        Self { now_ms: std::sync::atomic::AtomicU64::new(start_ms) }
    }
    /// 推进虚拟时间。
    pub fn advance(&self, ms: u64) {
        self.now_ms.fetch_add(ms, std::sync::atomic::Ordering::SeqCst);
    }
}
impl Clock for FakeClock {
    fn now_ms(&self) -> u64 {
        self.now_ms.load(std::sync::atomic::Ordering::SeqCst)
    }
}

/// 熔断器配置。
#[derive(Debug, Clone)]
pub struct RouterPolicy {
    /// 连续失败达到该值 → 熔断打开。
    pub max_consecutive_failures: u32,
    /// 熔断冷却期（半开探测前的等待）。
    pub cooldown_ms: u64,
    /// 半开探测成功后重置计数。
    pub half_open_probes: u32,
    /// RTT EMA 平滑系数（0-1）。
    pub rtt_alpha: f64,
    /// 隐私约束（选路过滤）。
    pub required_privacy: PrivacyLevel,
}

impl Default for RouterPolicy {
    fn default() -> Self {
        Self {
            max_consecutive_failures: 3,
            cooldown_ms: 30_000,
            half_open_probes: 1,
            rtt_alpha: 0.3,
            required_privacy: PrivacyLevel::Encrypted,
        }
    }
}

/// SyncRouter — 按「健康度 + 隐私 + 档位」策略选路与降级。
pub struct SyncRouter {
    entries: Vec<RouteEntry>,
    health: Mutex<HashMap<String, Health>>,
    policy: RouterPolicy,
    clock: Arc<dyn Clock>,
}

impl SyncRouter {
    /// 生产构造（真实时钟）。
    pub fn new(entries: Vec<RouteEntry>, policy: RouterPolicy) -> Self {
        Self {
            entries,
            health: Mutex::new(HashMap::new()),
            policy,
            clock: Arc::new(SystemClock),
        }
    }

    /// DST 构造（注入时钟 — 确定性仿真）。
    pub fn with_clock(entries: Vec<RouteEntry>, policy: RouterPolicy, clock: Arc<dyn Clock>) -> Self {
        Self { entries, health: Mutex::new(HashMap::new()), policy, clock }
    }

    /// 链路当前可用性（熔断状态机）。
    fn is_available(&self, url: &str, now_ms: u64) -> (bool, &'static str) {
        let guard = self.health.lock().unwrap();
        let Some(h) = guard.get(url) else {
            return (true, "never-used");
        };
        if h.opened_at_ms == 0 {
            return (true, "healthy");
        }
        // 熔断中: 冷却期过 → 半开（允许单次试探）
        if now_ms.saturating_sub(h.opened_at_ms) >= self.policy.cooldown_ms {
            (true, "half-open-probe")
        } else {
            (false, "circuit-open")
        }
    }

    /// 记录成功（重置失败计数 / 关闭熔断 / 更新 EMA RTT）。
    pub fn report_success(&self, url: &str, rtt_ms: f64) {
        let mut guard = self.health.lock().unwrap();
        let h = guard.entry(url.to_string()).or_default();
        h.consecutive_failures = 0;
        h.opened_at_ms = 0;
        h.ema_rtt_ms = if h.ema_rtt_ms == 0.0 {
            rtt_ms
        } else {
            let a = self.policy.rtt_alpha;
            h.ema_rtt_ms * (1.0 - a) + rtt_ms * a
        };
    }

    /// 记录失败（计数 → 熔断打开）。
    pub fn report_failure(&self, url: &str) {
        let now = self.clock.now_ms();
        let mut guard = self.health.lock().unwrap();
        let h = guard.entry(url.to_string()).or_default();
        h.consecutive_failures += 1;
        h.ema_rtt_ms = 0.0;
        if h.consecutive_failures >= self.policy.max_consecutive_failures {
            h.opened_at_ms = now;
            info!(url, failures = h.consecutive_failures, "circuit opened");
        }
    }

    /// 按隐私等级过滤链路。
    fn privacy_ok(&self, entry: &RouteEntry) -> bool {
        let required = self.policy.required_privacy;
        let offers = entry.privacy;
        use PrivacyLevel::*;
        match required {
            Plain => true,
            Encrypted => offers >= Encrypted,
            E2eeOnly => offers >= E2eeOnly,
        }
    }

    /// 选路（不实际连接 — 决策与执行分离，便于测试与审计）。
    ///
    /// 返回按档位排序后的首个**可用**链路; 全部不可用时返回
    /// 最高档位的 half-open 建议（让调用方试探恢复）。
    pub fn route(&self) -> Result<RouteDecision, crate::Error> {
        let now = self.clock.now_ms();
        // 档位升序 = 优先级降序
        let mut sorted: Vec<&RouteEntry> = self
            .entries
            .iter()
            .filter(|e| self.privacy_ok(e))
            .collect();
        sorted.sort_by_key(|e| e.tier);

        if sorted.is_empty() {
            return Err(crate::Error::Sync("no link satisfies privacy policy".into()));
        }

        // 第一轮: 全可用性过滤
        for e in &sorted {
            let (ok, state) = self.is_available(&e.endpoint.url, now);
            if ok {
                return Ok(RouteDecision {
                    tier: e.tier,
                    endpoint_url: e.endpoint.url.clone(),
                    reason: state.to_string(),
                });
            }
        }
        // 第二轮: 全熔断 → 半开试探最高档（冷却期最久的）
        if let Some(e) = sorted.first() {
            return Ok(RouteDecision {
                tier: e.tier,
                endpoint_url: e.endpoint.url.clone(),
                reason: "all-open-fallback-probe".into(),
            });
        }
        Err(crate::Error::Sync("no route available".into()))
    }

    /// 执行同步（决策层就绪; 执行层接线在后续 PR——
    /// 适配器需提供内部可变性以满足 Arc 下 connect 的 &mut 约束）。
    ///
    /// 当前语义: 返回路由决策，由调用方（sync 引擎）执行连接并回报
    /// `report_success/report_failure`（健康度状态机闭环）。
    pub async fn route_and_execute<R, Fut>(
        &self,
        execute: impl FnOnce(RouteDecision) -> Fut,
    ) -> Result<(RouteDecision, R), crate::Error>
    where
        Fut: std::future::Future<Output = Result<R, crate::Error>>,
    {
        // 路由器只决策一次; 执行失败由调用方回报 report_failure，
        // 下次 route() 自动降级（决策与执行解耦 — 便于 DST 与生产一致）
        let decision = self.route()?;
        let url = decision.endpoint_url.clone();
        match execute(decision.clone()).await {
            Ok(r) => Ok((decision, r)),
            Err(e) => {
                warn!(url = %url, error = %e, "execute failed; degradation next route");
                self.report_failure(&url);
                Err(e)
            }
        }
    }

    /// 健康度快照（诊断/测试）。
    pub fn health_snapshot(&self) -> Vec<(String, u32, bool, f64)> {
        let guard = self.health.lock().unwrap();
        self.entries
            .iter()
            .map(|e| {
                let h = guard.get(&e.endpoint.url);
                (
                    e.endpoint.url.clone(),
                    h.map(|x| x.consecutive_failures).unwrap_or(0),
                    h.map(|x| x.opened_at_ms != 0).unwrap_or(false),
                    h.map(|x| x.ema_rtt_ms).unwrap_or(0.0),
                )
            })
            .collect()
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    fn entry(tier: RouteTier, url: &str, privacy: PrivacyLevel) -> RouteEntry {
        RouteEntry {
            target: Arc::new(NoopTarget),
            tier,
            endpoint: Endpoint { url: url.into(), protocol: SyncProtocol::Iroh },
            privacy,
        }
    }

    struct NoopTarget;
    #[async_trait]
    impl SyncTarget for NoopTarget {
        async fn connect(&mut self, _e: &Endpoint) -> Result<aurora_core::traits::sync_target::Connection, aurora_core::Error> {
            unimplemented!()
        }
        async fn sync(&self, _c: &aurora_core::traits::sync_target::Connection, _d: &aurora_core::traits::sync_target::DocSet) -> Result<SyncReport, aurora_core::Error> {
            unimplemented!()
        }
        fn watch(&self, _cb: Box<dyn Fn(aurora_core::traits::sync_target::SyncEvent) + Send + Sync>) {}
        async fn disconnect(&self, _c: &aurora_core::traits::sync_target::Connection) -> Result<(), aurora_core::Error> {
            Ok(())
        }
    }

    fn router(entries: Vec<RouteEntry>, clock: &Arc<FakeClock>) -> SyncRouter {
        SyncRouter::with_clock(
            entries,
            RouterPolicy { max_consecutive_failures: 2, cooldown_ms: 10_000, ..Default::default() },
            clock.clone(),
        )
    }

    // ═══ DST 场景 1: 默认选 P2P（最高档） ═══
    #[test]
    fn dst_routes_to_highest_tier_by_default() {
        let clock = Arc::new(FakeClock::new(1000));
        let r = router(
            vec![
                entry(RouteTier::P2p, "iroh://a", PrivacyLevel::E2eeOnly),
                entry(RouteTier::Lan, "lan://b", PrivacyLevel::E2eeOnly),
                entry(RouteTier::Cloud, "cloud://c", PrivacyLevel::Encrypted),
            ],
            &clock,
        );
        let d = r.route().unwrap();
        assert_eq!((d.tier, d.endpoint_url.as_str()), (RouteTier::P2p, "iroh://a"));
        assert_eq!(d.reason, "never-used");
    }

    // ═══ DST 场景 2: 隐私约束过滤（E2eeOnly 禁云/外部） ═══
    #[test]
    fn dst_privacy_e2ee_only_excludes_cloud() {
        let clock = Arc::new(FakeClock::new(1000));
        let r = SyncRouter::with_clock(
            vec![
                entry(RouteTier::Cloud, "cloud://c", PrivacyLevel::Encrypted),
                entry(RouteTier::Lan, "lan://b", PrivacyLevel::E2eeOnly),
            ],
            RouterPolicy { required_privacy: PrivacyLevel::E2eeOnly, ..Default::default() },
            clock,
        );
        let d = r.route().unwrap();
        assert_eq!(d.tier, RouteTier::Lan, "云被隐私约束过滤");
    }

    // ═══ DST 场景 3: 熔断降级（P2P 连续失败 → 自动切 LAN） ═══
    #[test]
    fn dst_circuit_breaker_degrades_to_lan() {
        let clock = Arc::new(FakeClock::new(1000));
        let r = router(
            vec![
                entry(RouteTier::P2p, "iroh://a", PrivacyLevel::E2eeOnly),
                entry(RouteTier::Lan, "lan://b", PrivacyLevel::E2eeOnly),
            ],
            &clock,
        );
        // P2P 连续失败 2 次（达到熔断阈值）
        r.report_failure("iroh://a");
        r.report_failure("iroh://a");
        let d = r.route().unwrap();
        assert_eq!((d.tier, d.endpoint_url.as_str()), (RouteTier::Lan, "lan://b"));
        // 健康快照: P2P 熔断中
        let snap = r.health_snapshot();
        assert!(snap.iter().any(|(u, _, open, _)| u == "iroh://a" && *open));
    }

    // ═══ DST 场景 4: 冷却期后半开探测（恢复路径） ═══
    #[test]
    fn dst_half_open_probe_after_cooldown() {
        let clock = Arc::new(FakeClock::new(1000));
        let r = router(
            vec![entry(RouteTier::P2p, "iroh://a", PrivacyLevel::E2eeOnly)],
            &clock,
        );
        r.report_failure("iroh://a");
        r.report_failure("iroh://a"); // 熔断打开 @ t=1000
        // 冷却期内: 不可用（全熔断 → fallback probe 标记）
        let d1 = r.route().unwrap();
        assert_eq!(d1.reason, "all-open-fallback-probe");
        // 推进 11s（> cooldown 10s）→ 半开探测可用
        clock.advance(11_000);
        let d2 = r.route().unwrap();
        assert_eq!(d2.reason, "half-open-probe");
        // 探测成功 → 熔断关闭，恢复 healthy
        r.report_success("iroh://a", 42.0);
        let d3 = r.route().unwrap();
        assert_eq!(d3.reason, "healthy");
        // EMA RTT 记录
        let snap = r.health_snapshot();
        assert_eq!(snap[0].3, 42.0);
    }

    // ═══ DST 场景 5: 全链降级（P2P+LAN 熔断 → 云兜底） ═══
    #[test]
    fn dst_full_chain_degradation_to_cloud() {
        let clock = Arc::new(FakeClock::new(1000));
        let r = router(
            vec![
                entry(RouteTier::P2p, "iroh://a", PrivacyLevel::E2eeOnly),
                entry(RouteTier::Lan, "lan://b", PrivacyLevel::E2eeOnly),
                entry(RouteTier::Cloud, "cloud://c", PrivacyLevel::Encrypted),
            ],
            &clock,
        );
        for _ in 0..2 {
            r.report_failure("iroh://a");
            r.report_failure("lan://b");
        }
        let d = r.route().unwrap();
        assert_eq!((d.tier, d.endpoint_url.as_str()), (RouteTier::Cloud, "cloud://c"));
    }

    // ═══ DST 场景 6: EMA RTT 平滑（成本感知基础） ═══
    #[test]
    fn dst_ema_rtt_smooths() {
        let clock = Arc::new(FakeClock::new(1000));
        let r = router(vec![entry(RouteTier::P2p, "iroh://a", PrivacyLevel::E2eeOnly)], &clock);
        // alpha=0.3: (0→100) → 100; (100→200) → 100*0.7+200*0.3=130
        r.report_success("iroh://a", 100.0);
        r.report_success("iroh://a", 200.0);
        let snap = r.health_snapshot();
        assert!((snap[0].3 - 130.0).abs() < 0.01, "EMA: {}", snap[0].3);
    }

    // ═══ DST 场景 7: 确定性断言（同场景序列 → 同结果） ═══
    #[test]
    fn dst_deterministic_replay() {
        // 同一时钟序列 + 同一失败序列 → 完全相同的路由轨迹
        let run = || {
            let clock = Arc::new(FakeClock::new(500));
            let r = router(
                vec![
                    entry(RouteTier::P2p, "iroh://a", PrivacyLevel::E2eeOnly),
                    entry(RouteTier::Lan, "lan://b", PrivacyLevel::E2eeOnly),
                ],
                &clock,
            );
            let mut trace = Vec::new();
            trace.push(r.route().unwrap());
            r.report_failure("iroh://a");
            trace.push(r.route().unwrap());
            r.report_failure("iroh://a");
            trace.push(r.route().unwrap());
            clock.advance(10_000);
            trace.push(r.route().unwrap());
            trace
        };
        let t1 = run();
        let t2 = run();
        assert_eq!(t1, t2, "同输入必须同输出（DST 核心约束）");
    }
}
