//! DK-45: 插件沙箱策略宿主入口——策略读写 command 层。
//!
//! 原语（traits/plugin_policy.rs，a9b039f）只读复用：
//! - `set_plugin_sandbox_policy` / `plugin_sandbox_policy`（publisher 维度 KV 持久化）
//! - `apply_sandbox_policy`（安装路径覆盖——市场安装接入点，本卡锚定语义）
//!
//! 领地：desktop/tauri（新文件）；core 原语零改动。

use crate::sync_commands::get_booted;
use aurora_core::traits::plugin_policy::{
    apply_sandbox_policy, plugin_sandbox_policy, set_plugin_sandbox_policy,
};
use aurora_core::traits::plugin_runtime::{PluginManifest, SandboxLimits};
use serde::Serialize;

/// 策略 DTO（列表行——含未配置项的缺省展示标记）。
#[derive(Debug, Serialize)]
pub struct PluginPolicyDto {
    pub publisher: String,
    pub epoch_deadline_ticks: u32,
    pub max_memory_bytes: usize,
    /// true = 宿主已显式配置；false = 未配置（展示缺省安全值）
    pub configured: bool,
}

/// 缺省安全值展示常量（与 SandboxLimits::default 一致——DTO 序列化用）。
fn default_limits() -> SandboxLimits {
    SandboxLimits::default()
}

/// 纯函数：给定 publisher 名单与 KV，产出策略列表（含缺省展示）。
/// command 层薄壳透传；单测直接覆盖此函数（DoD 1）。
///
/// # Errors
/// KV 读取失败（透传）。
pub async fn build_policy_list(
    kv: &dyn aurora_core::traits::kv_store::KVStore,
    publishers: &[String],
) -> Result<Vec<PluginPolicyDto>, String> {
    let def = default_limits();
    let mut out = Vec::with_capacity(publishers.len());
    for p in publishers {
        let policy = plugin_sandbox_policy(kv, p)
            .await
            .map_err(|e| e.to_string())?;
        match policy {
            Some(l) => out.push(PluginPolicyDto {
                publisher: p.clone(),
                epoch_deadline_ticks: l.epoch_deadline_ticks,
                max_memory_bytes: l.max_memory_bytes,
                configured: true,
            }),
            None => out.push(PluginPolicyDto {
                publisher: p.clone(),
                epoch_deadline_ticks: def.epoch_deadline_ticks,
                max_memory_bytes: def.max_memory_bytes,
                configured: false,
            }),
        }
    }
    Ok(out)
}

/// 列出插件沙箱策略（`publishers` 为前端传入的已装插件作者名单；
/// 未配置项按缺省安全值展示并标记 `configured=false`）。
///
/// # Errors
/// 应用未初始化 / KV 读取失败。
#[tauri::command]
pub async fn cmd_list_plugin_policies(
    publishers: Vec<String>,
) -> Result<Vec<PluginPolicyDto>, String> {
    let booted = get_booted()?;
    build_policy_list(booted.core.kv_store.as_ref(), &publishers).await
}

/// 设置/更新某 publisher 的沙箱限额策略（编辑即写——与 wifi_only 同语义）。
///
/// `0` 值合法（DK-42 语义：invoke 侧回退 runtime 缺省）。
///
/// # Errors
/// 应用未初始化 / publisher 为空 / KV 写入失败。
#[tauri::command]
pub async fn cmd_set_plugin_policy(
    publisher: String,
    epoch_deadline_ticks: u32,
    max_memory_bytes: usize,
) -> Result<(), String> {
    let booted = get_booted()?;
    set_plugin_sandbox_policy(
        booted.core.kv_store.as_ref(),
        &publisher,
        SandboxLimits {
            epoch_deadline_ticks,
            max_memory_bytes,
        },
    )
    .await
    .map_err(|e| e.to_string())
}

/// 安装路径语义（DK-14 市场接入点锚定——command 层透传供 e2e 语义复验；
/// 真实安装流程在市场迭代卡调用此语义）。
///
/// # Errors
/// publisher 为空 / KV 读取失败。
#[tauri::command]
pub async fn cmd_apply_plugin_policy(
    publisher: String,
    manifest: PluginManifest,
) -> Result<PluginManifest, String> {
    let booted = get_booted()?;
    let policy = plugin_sandbox_policy(booted.core.kv_store.as_ref(), &publisher)
        .await
        .map_err(|e| e.to_string())?;
    let mut m = manifest;
    apply_sandbox_policy(&mut m, policy);
    Ok(m)
}
