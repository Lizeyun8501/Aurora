//! 同步网络门 (Sync Network Gate) — DK-08 §7.3「仅 Wi-Fi 同步」同步侧部分。
//!
//! # 职责切分（详见 `issues/wf/bravo-request-wifi-only-sync.md`）
//!
//! - **本模块（Bravo 领地，已交付）**：网络分类、策略引擎、运行时开关。
//!   同步入口（SyncRouter / 各引擎）在发起传输前调用
//!   [`SyncGate::evaluate`]，`Defer*` 决策应转交 offline_queue 延迟重试。
//! - **平台侧（Alpha，待排期）**：实现 [`NetworkStateProvider`] ——
//!   Android `ConnectivityManager`（NET_CAPABILITY_NOT_METERED）、
//!   iOS `NWPathMonitor`（`path.isExpensive`）、桌面可用工作区已内置的
//!   netwatch `netmon`（`State.is_expensive`，注意该字段并非全平台填充，
//!   未填充时桌面默认 [`NetworkClass::Unmetered`]）。
//!
//! # 语义
//!
//! - 策略拒绝**不是错误**：返回 [`GateDecision::Defer*`]，由调用方决定
//!   入队重试（避免把策略当故障污染重试/熔断统计）；
//! - 设置运行时可切换（设置页改动无需重建门）；
//! - `Offline` 恒推迟，与「仅 Wi-Fi」开关无关。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// 平台网络连接分类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkClass {
    /// 不计量网络（Wi-Fi / 以太网）。
    Unmetered,
    /// 计量网络（蜂窝 / 热点 / 付费流量）。
    Metered,
    /// 无网络。
    Offline,
}

/// 平台网络状态源（mobile-ffi / 桌面壳实现；平台接入见 request 文档）。
///
/// 实现应返回**缓存的最新状态**（平台回调刷新），而非同步发起系统调用
/// —— 本 trait 会在每次同步决策时被调用，必须廉价。
pub trait NetworkStateProvider: Send + Sync {
    fn current_class(&self) -> NetworkClass;
}

/// 同步门决策。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateDecision {
    /// 允许同步。
    Allow,
    /// 仅 Wi-Fi 开启且当前为计量网络 → 推迟到不计量网络。
    DeferUntilUnmetered,
    /// 当前无网络 → 推迟。
    DeferUntilOnline,
}

impl GateDecision {
    /// 是否放行（drain 门禁判定用——`Defer*` 一律不放行，DK-51 案二）。
    pub fn is_allowed(self) -> bool {
        matches!(self, GateDecision::Allow)
    }

    /// 人话原因（两端 drain 拒绝错误信息共用——decision 之外携带
    /// 可读解释，前端直接展示）。
    pub fn reason(self) -> &'static str {
        match self {
            GateDecision::Allow => "策略允许同步",
            GateDecision::DeferUntilUnmetered => "仅 Wi-Fi 同步已开启，当前为计量网络",
            GateDecision::DeferUntilOnline => "当前无网络连接",
        }
    }
}

/// 仅 Wi-Fi 同步门（运行时可切换设置）。
pub struct SyncGate {
    wifi_only: AtomicBool,
    provider: Arc<dyn NetworkStateProvider>,
}

impl SyncGate {
    /// 构造同步门。`wifi_only` 为用户设置初值。
    pub fn new(provider: Arc<dyn NetworkStateProvider>, wifi_only: bool) -> Self {
        Self {
            wifi_only: AtomicBool::new(wifi_only),
            provider,
        }
    }

    /// 运行时切换「仅 Wi-Fi」设置（设置页调用）。
    pub fn set_wifi_only(&self, on: bool) {
        self.wifi_only.store(on, Ordering::SeqCst);
    }

    /// 当前设置值。
    pub fn wifi_only(&self) -> bool {
        self.wifi_only.load(Ordering::SeqCst)
    }

    /// 评估当前网络下是否允许同步。
    pub fn evaluate(&self) -> GateDecision {
        match self.provider.current_class() {
            NetworkClass::Offline => GateDecision::DeferUntilOnline,
            NetworkClass::Metered if self.wifi_only() => GateDecision::DeferUntilUnmetered,
            _ => GateDecision::Allow,
        }
    }
}

/// 桌面默认平台源：恒不计量（netwatch `is_expensive` 平台差异未验证前
/// 按 request 裁决 09-20 默认放行——桌面「仅 Wi-Fi」开关关闭时 provider
/// 实际不被消费；未来接 netwatch 时替换此实现）。
pub struct AlwaysUnmetered;

impl NetworkStateProvider for AlwaysUnmetered {
    fn current_class(&self) -> NetworkClass {
        NetworkClass::Unmetered
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 可变状态测试源（模拟平台回调刷新）。
    #[derive(Default)]
    struct FakeProvider {
        class: std::sync::atomic::AtomicU8,
    }

    impl FakeProvider {
        fn set(&self, c: NetworkClass) {
            let v = match c {
                NetworkClass::Unmetered => 0,
                NetworkClass::Metered => 1,
                NetworkClass::Offline => 2,
            };
            self.class.store(v, Ordering::SeqCst);
        }
    }

    impl NetworkStateProvider for FakeProvider {
        fn current_class(&self) -> NetworkClass {
            match self.class.load(Ordering::SeqCst) {
                1 => NetworkClass::Metered,
                2 => NetworkClass::Offline,
                _ => NetworkClass::Unmetered,
            }
        }
    }

    fn gate(wifi_only: bool) -> (SyncGate, Arc<FakeProvider>) {
        let provider = Arc::new(FakeProvider::default());
        (SyncGate::new(provider.clone(), wifi_only), provider)
    }

    /// DK-08 §7.3 验收 1：仅 Wi-Fi 开启 + 计量网络 → 推迟。
    #[test]
    fn gate_defers_on_metered_when_wifi_only() {
        let (gate, provider) = gate(true);
        provider.set(NetworkClass::Metered);
        assert_eq!(gate.evaluate(), GateDecision::DeferUntilUnmetered);
    }

    /// DK-08 §7.3 验收 2：仅 Wi-Fi 关闭 + 计量网络 → 允许。
    #[test]
    fn gate_allows_metered_when_wifi_only_off() {
        let (gate, provider) = gate(false);
        provider.set(NetworkClass::Metered);
        assert_eq!(gate.evaluate(), GateDecision::Allow);
    }

    /// DK-08 §7.3 验收 3：Offline 恒推迟（与开关无关）。
    #[test]
    fn gate_defers_offline_regardless_of_setting() {
        let (gate, provider) = gate(false);
        provider.set(NetworkClass::Offline);
        assert_eq!(gate.evaluate(), GateDecision::DeferUntilOnline);
        gate.set_wifi_only(true);
        assert_eq!(gate.evaluate(), GateDecision::DeferUntilOnline);
    }

    /// DK-08 §7.3 验收 4：不计量网络恒允许；运行时开关即时生效。
    #[test]
    fn gate_unmetered_allows_and_toggle_is_live() {
        let (gate, provider) = gate(false);
        provider.set(NetworkClass::Unmetered);
        assert_eq!(gate.evaluate(), GateDecision::Allow);

        // 切到计量 + 开启仅 Wi-Fi → 推迟；关掉 → 放行（无需重建门）
        provider.set(NetworkClass::Metered);
        gate.set_wifi_only(true);
        assert!(gate.wifi_only());
        assert_eq!(gate.evaluate(), GateDecision::DeferUntilUnmetered);
        gate.set_wifi_only(false);
        assert_eq!(gate.evaluate(), GateDecision::Allow);
    }

    /// 桌面默认源：恒 Unmetered（Allow）——wifi_only=ON 也不推迟
    ///（netwatch 未接入前的裁决口径）。
    #[test]
    fn always_unmetered_provider_allows() {
        let gate = SyncGate::new(Arc::new(AlwaysUnmetered), true);
        assert_eq!(gate.evaluate(), GateDecision::Allow);
    }

    /// DK-51 案二：evaluate 组合矩阵全覆盖（网络态 × wifi_only）。
    /// Offline 恒推迟优先于 wifi_only（模块语义：与「仅 Wi-Fi」开关无关）。
    #[test]
    fn gate_matrix_all_combinations() {
        // wifi_only=false：仅 Offline 推迟，其余放行
        let (g, p) = gate(false);
        p.set(NetworkClass::Offline);
        assert_eq!(g.evaluate(), GateDecision::DeferUntilOnline);
        p.set(NetworkClass::Metered);
        assert_eq!(
            g.evaluate(),
            GateDecision::Allow,
            "计量+wifi_only=false=不推迟"
        );
        p.set(NetworkClass::Unmetered);
        assert_eq!(g.evaluate(), GateDecision::Allow);

        // wifi_only=true：Offline 恒推迟 > 计量推迟 > 不计量放行
        let (g, p) = gate(true);
        p.set(NetworkClass::Offline);
        assert_eq!(
            g.evaluate(),
            GateDecision::DeferUntilOnline,
            "Offline 优先于 wifi_only"
        );
        p.set(NetworkClass::Metered);
        assert_eq!(g.evaluate(), GateDecision::DeferUntilUnmetered);
        p.set(NetworkClass::Unmetered);
        assert_eq!(g.evaluate(), GateDecision::Allow);

        // helper 一致性（drain 门禁判定契约）
        assert!(GateDecision::Allow.is_allowed());
        assert!(!GateDecision::DeferUntilOnline.is_allowed());
        assert!(!GateDecision::DeferUntilUnmetered.is_allowed());
        assert_eq!(GateDecision::DeferUntilOnline.reason(), "当前无网络连接");
        assert_eq!(
            GateDecision::DeferUntilUnmetered.reason(),
            "仅 Wi-Fi 同步已开启，当前为计量网络"
        );
    }
}
