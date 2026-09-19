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
}
