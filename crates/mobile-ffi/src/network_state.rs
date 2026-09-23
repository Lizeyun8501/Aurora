//! Android 网络状态源（DK-08 §7.3 — 非 feature-gated：SyncGate 装配
//! 全构建形态可用；iroh P2P 引擎见 p2p_sync，与本模块解耦）。
//!
//! 从 p2p_sync.rs 拆出（2026-09-24 Alpha 装配切片）：aurora-sync 转
//! mobile-ffi 必选依赖后，provider 需在无 p2p-sync feature 时也可用。

use aurora_sync::sync_gate::{NetworkClass, NetworkStateProvider};
use jni::objects::JClass;
use jni::sys::jint;
use jni::JNIEnv;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

// ===========================================================================
// NetworkStateProvider — DK-08 §7.3「仅 Wi-Fi 同步」平台网络状态源
// （request `bravo-request-wifi-only-sync` 申请 1 裁决：ConnectivityManager
//   缓存方案，Kotlin 推送 / Rust 读缓存）
// ===========================================================================

/// Android 网络状态缓存（进程级单例）。
///
/// 数据链路：Kotlin 侧 `ConnectivityManager.registerNetworkCallback` 在
/// `onCapabilitiesChanged`（NET_CAPABILITY_NOT_METERED）与 `onLost` 时调
/// `SyncEngine.nativeUpdateNetworkState(state)` 推送最新分类；
/// Rust 侧 `current_class()` 为原子读（廉价、同步、不跨 JNI 阻塞），
/// 与 sync_gate.rs「读缓存 + 不污染熔断统计」的语义约定一致。
pub struct AndroidNetworkState {
    /// 0 = Unmetered / 1 = Metered / 2 = Offline（越界按 0 处理：默认放行）。
    cached: AtomicU8,
}

/// 进程级单例（JNI 回调与 SyncGate 装配共享）。
pub static NETWORK_STATE: AndroidNetworkState = AndroidNetworkState {
    cached: AtomicU8::new(0),
};

pub const CLASS_UNMETERED: u8 = 0;
pub const CLASS_METERED: u8 = 1;
pub const CLASS_OFFLINE: u8 = 2;

impl AndroidNetworkState {
    /// JNI 推送入口（`nativeUpdateNetworkState` 调用；越界值归零 = 放行）。
    pub fn update(&self, raw: u8) {
        let v = match raw {
            CLASS_METERED => CLASS_METERED,
            CLASS_OFFLINE => CLASS_OFFLINE,
            _ => CLASS_UNMETERED,
        };
        self.cached.store(v, Ordering::Relaxed);
    }
}

impl NetworkStateProvider for AndroidNetworkState {
    fn current_class(&self) -> NetworkClass {
        match self.cached.load(Ordering::Relaxed) {
            CLASS_METERED => NetworkClass::Metered,
            CLASS_OFFLINE => NetworkClass::Offline,
            _ => NetworkClass::Unmetered,
        }
    }
}

/// Kotlin 回调桥：`SyncEngine.nativeUpdateNetworkState(state: Int)`。
///
/// Kotlin 侧约定：0 = NOT_METERED、1 = METERED、2 = 网络丢失（Offline）。
#[no_mangle]
pub extern "system" fn Java_com_aurora_note_SyncEngine_nativeUpdateNetworkState(
    _env: JNIEnv,
    _class: JClass,
    state: jint,
) {
    NETWORK_STATE.update(state.clamp(0, 255) as u8);
}

/// SyncGate 装配入口：`Arc<dyn NetworkStateProvider>`（Bravo 接 SyncGate 时注入）。
pub fn network_state_provider() -> Arc<dyn NetworkStateProvider> {
    Arc::new(AndroidNetworkStateProxy)
}

/// [`NETWORK_STATE`] 的 trait-object 视图（`current_class` 委托单例）。
struct AndroidNetworkStateProxy;

impl NetworkStateProvider for AndroidNetworkStateProxy {
    fn current_class(&self) -> NetworkClass {
        NETWORK_STATE.current_class()
    }
}

#[cfg(test)]
mod network_state_tests {
    use super::*;

    /// request 验收清单 1：三态映射正确（计量/不计量/离线）。
    #[test]
    fn maps_all_three_classes() {
        let st = AndroidNetworkState {
            cached: AtomicU8::new(CLASS_UNMETERED),
        };
        st.update(CLASS_UNMETERED);
        assert_eq!(st.current_class(), NetworkClass::Unmetered);
        st.update(CLASS_METERED);
        assert_eq!(st.current_class(), NetworkClass::Metered);
        st.update(CLASS_OFFLINE);
        assert_eq!(st.current_class(), NetworkClass::Offline);
    }

    /// 越界 raw 值归零（默认放行语义，拒绝不成立时不误伤）。
    #[test]
    fn out_of_range_raw_falls_back_to_unmetered() {
        let st = AndroidNetworkState {
            cached: AtomicU8::new(CLASS_METERED),
        };
        st.update(200);
        assert_eq!(st.current_class(), NetworkClass::Unmetered);
    }

    /// trait-object 装配路径可用（SyncGate::new 的签名契约）。
    #[test]
    fn provider_assembles_as_trait_object() {
        let provider: Arc<dyn NetworkStateProvider> = network_state_provider();
        // 默认缓存 = Unmetered（进程启动到首条回调之间不阻塞同步）
        assert_eq!(provider.current_class(), NetworkClass::Unmetered);
        NETWORK_STATE.update(CLASS_METERED);
        assert_eq!(provider.current_class(), NetworkClass::Metered);
        NETWORK_STATE.update(CLASS_UNMETERED);
    }
}
