//! DK-08 §7.3 同步门 command 层（wifi_only 设置端到端 · Alpha 装配切片
//! 2026-09-24）。
//!
//! # 设计
//! - 读写经 `BootedApp` 同源语义：KV 权威（`settings:sync.wifi_only`，
//!   崩溃安全）+ `SyncGate::set_wifi_only` 运行时即时生效；
//! - BOOTED 句柄复用 APP_STATE——gate 与 core 同源装配，读 KV 用 core
//!   的 kv_store（BOOTED 生命周期与 APP_STATE 一致）。

use aurora_bootstrap::BootedApp;
use std::sync::{Arc, Mutex};

/// bootstrap 结果的进程级缓存（wifi_only 读写需 BootedApp 方法；
/// setup 阶段与 APP_STATE 同步注入）。
pub(crate) static BOOTED_STATE: Mutex<Option<Arc<BootedApp>>> = Mutex::new(None);

fn get_booted() -> Result<Arc<BootedApp>, String> {
    BOOTED_STATE
        .lock()
        .expect("BOOTED_STATE mutex poisoned")
        .clone()
        .ok_or_else(|| "应用尚未初始化".into())
}

/// 查询「仅 Wi-Fi 同步」开关（KV 权威值 + 门内当前值一致性由装配保证）。
///
/// # Errors
/// 应用未初始化 / KV 读取失败（透传）。
#[tauri::command]
pub async fn cmd_get_wifi_only() -> Result<bool, String> {
    let booted = get_booted()?;
    booted.wifi_only().map_err(|e| e.to_string())
}

/// 设置「仅 Wi-Fi 同步」开关（KV 持久化 + 门运行时切换，即时生效）。
///
/// # Errors
/// 应用未初始化 / KV 写入失败（透传）。
#[tauri::command]
pub async fn cmd_set_wifi_only(on: bool) -> Result<(), String> {
    let booted = get_booted()?;
    booted.set_wifi_only(on).map_err(|e| e.to_string())
}
