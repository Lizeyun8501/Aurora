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

// ===== 桌面安装面接线（DK-市场卡）：installer 语义层薄委托 =====
//
// 领地纪律：fail-closed 语义全部在 aurora-plugin::installer（纯函数可单测）；
// 本层只做 IO（wasm 文件读取）+ 状态持有 + 错误转 String。

use std::sync::Arc;
use tauri::State;

use aurora_plugin::fetcher::{HttpFetcher, PluginFetcher};
use aurora_plugin::installer;
use aurora_plugin::marketplace::Marketplace;
use aurora_plugin::wasm_runtime::WasmRuntime;

/// 插件运行时状态（setup 时 manage；market 注入 SecurityCryptoProvider）。
/// Clone：hourly tick 需 clone 进 async spawn（成员均 Arc——浅拷贝共享）。
#[derive(Clone)]
pub struct PluginState {
    pub runtime: Arc<WasmRuntime>,
    pub market: Arc<Marketplace>,
    /// 市场下载通道（DK-市场下载器：桌面默认 Http；测试/离线可注入 Mock）
    pub fetcher: Arc<dyn PluginFetcher>,
    /// DK-1.1 接线：热更新状态机（统一注册表语义——内存表退化为会话缓存，
    /// 权威源=installer KV。桌面安装件升级流入口）
    pub hot: Arc<aurora_plugin::hot_update::HotUpdateManager>,
}

impl PluginState {
    pub fn new() -> Self {
        Self {
            runtime: Arc::new(WasmRuntime::with_default_registry()),
            market: Arc::new(Marketplace::new(Arc::new(
                aurora_security::crypto_provider_impl::SecurityCryptoProvider::new(),
            ))),
            fetcher: Arc::new(HttpFetcher::default()),
            hot: Arc::new(aurora_plugin::hot_update::HotUpdateManager::new()),
        }
    }
}

impl Default for PluginState {
    fn default() -> Self {
        Self::new()
    }
}

/// 审计即时 flush（审计 flush 定量——挂起池项）：
/// 安装面变更（install/uninstall 成功或六环拒绝）后立即落盘审计快照——
/// 这些事件低频高价值（安全审计主对象），不等 hourly tick（丢失窗口
/// 从 1h 收紧到即时）。失败不阻断主流程（warn——hourly tick 兜底重试）。
async fn flush_audit_after(
    runtime: &aurora_plugin::wasm_runtime::WasmRuntime,
    kv: &dyn aurora_core::traits::kv_store::KVStore,
    op: &str,
) {
    match aurora_plugin::audit::persist_audit(runtime, kv).await {
        Ok(n) if n > 0 => tracing::debug!(n, op, "audit flushed after {op}"),
        Ok(_) => {}
        Err(e) => {
            tracing::warn!(error = %e, op, "audit flush after {op} failed; hourly tick will retry")
        }
    }
}

/// 安装插件（市场 listing + manifest + 本地 wasm 文件路径）。
///
/// 本地开发安装（六环 fail-closed 链在 installer（listing 存在性/id 一致/checksum/Plugin::new
/// 强签名/register/KV 持久化）；wasm 字节从 `wasm_path` 读入（市场下载器
/// 另卡——当前本地开发安装语义）。
///
/// # Errors
/// 文件读取失败 / installer 任一环拒绝（错误串透传）。
#[tauri::command]
pub async fn cmd_plugin_install(
    state: State<'_, PluginState>,
    listing_id: String,
    manifest: PluginManifest,
    wasm_path: String,
    reinstall: Option<bool>,
) -> Result<PluginManifest, String> {
    let booted = crate::sync_commands::get_booted()?;
    let wasm_bytes = tokio::fs::read(&wasm_path)
        .await
        .map_err(|e| format!("read wasm `{wasm_path}`: {e}"))?;
    let result = installer::install_plugin(
        &state.runtime,
        &booted.core.kv_store,
        &state.market,
        &listing_id,
        manifest,
        &wasm_bytes,
        reinstall.unwrap_or(false),
    )
    .await;
    // 成功与六环拒绝（denied 审计行）都即时落盘
    flush_audit_after(
        &state.runtime,
        booted.core.kv_store.as_ref(),
        "plugin_install",
    )
    .await;
    result.map_err(|e| e.to_string())
}

/// 已安装插件清单（KV 前缀扫描；损坏条目上抛不静默）。
///
/// # Errors
/// 应用未初始化 / KV 读失败 / 元数据损坏。
#[tauri::command]
pub async fn cmd_plugin_list() -> Result<Vec<PluginManifest>, String> {
    let booted = crate::sync_commands::get_booted()?;
    installer::list_installed(&booted.core.kv_store)
        .await
        .map_err(|e| e.to_string())
}

/// 市场可用件清单（listing 全量——UI 市场分区数据源）。
///
/// # Errors
/// 应用未初始化（market 状态在 PluginState，无需 boot）。
#[tauri::command]
pub fn cmd_market_list(state: State<'_, PluginState>) -> Vec<aurora_plugin::PluginListing> {
    state.market.list()
}

/// 市场安装（DK-市场下载器）：fetcher 下载 → 六环复用。
///
/// `require_verified=true`（默认）：Official/ThirdParty 未 verified 拒装
/// fail-closed；Local 豁免。wasm 字节来源=listing.download_url（HttpFetcher
/// 32MiB/30s fail-closed）。
///
/// # Errors
/// listing 不存在 / 未 verified 拒装 / 下载失败（超限/超时/非 2xx）/
/// installer 任一环拒绝（错误串透传）。
#[tauri::command]
pub async fn cmd_market_install(
    state: State<'_, PluginState>,
    listing_id: String,
    manifest: Option<PluginManifest>,
    require_verified: Option<bool>,
    reinstall: Option<bool>,
) -> Result<PluginManifest, String> {
    let booted = crate::sync_commands::get_booted()?;
    let result = installer::install_from_market(
        &state.runtime,
        &booted.core.kv_store,
        &state.market,
        state.fetcher.as_ref(),
        &listing_id,
        manifest,
        aurora_plugin::installer::MarketInstallPolicy {
            require_verified: require_verified.unwrap_or(true),
            reinstall: reinstall.unwrap_or(false),
        },
    )
    .await;
    // 成功与六环拒绝（denied 审计行）都即时落盘
    flush_audit_after(
        &state.runtime,
        booted.core.kv_store.as_ref(),
        "market_install",
    )
    .await;
    result.map_err(|e| e.to_string())
}

/// 卸载插件（runtime 反注册 + shares 双向清理（DK-17.1）+ KV 两键清除）。
///
/// # Errors
/// 未安装 / KV 删除失败。
#[tauri::command]
pub async fn cmd_plugin_uninstall(
    state: State<'_, PluginState>,
    plugin_id: String,
) -> Result<(), String> {
    let booted = crate::sync_commands::get_booted()?;
    let result =
        installer::uninstall_plugin(&state.runtime, &booted.core.kv_store, &plugin_id).await;
    flush_audit_after(
        &state.runtime,
        booted.core.kv_store.as_ref(),
        "plugin_uninstall",
    )
    .await;
    result.map_err(|e| e.to_string())
}

/// 插件方法调用（宿主函数面 or guest 导出面——runtime 既有语义透传）。
///
/// # Errors
/// 未注册 / 未知方法 / 能力拒绝 / 沙箱执行错误。
#[tauri::command]
pub async fn cmd_plugin_invoke(
    state: State<'_, PluginState>,
    plugin_id: String,
    method: String,
    args: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let runtime = state.runtime.clone();
    let booted = crate::sync_commands::get_booted()?;
    // DK-模块编译缓存：prime 在 async 面先行（缓存命中→deserialize 路径；
    // miss→全量编译+懒回写 KV——二次启动起免编译）。fail-open：prime
    // Err 仅降级为正常 invoke（缓存是优化非正确性依赖）。
    if let Some(bytes) = runtime.component_bytes(&plugin_id) {
        let _ = aurora_plugin::cache::load_component_cached(
            &runtime,
            &*booted.core.kv_store,
            &plugin_id,
            &bytes,
            "",
        )
        .await;
    }
    // wasmtime 同步执行放 blocking 线程（epoch 中断依赖 ticker 线程，不受影响）
    tokio::task::spawn_blocking(move || {
        runtime
            .invoke(&plugin_id, &method, &args)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

/// 插件审计链查询（DK-审计面：内存环形缓冲最近 N 条，可按插件过滤——
/// stderr 双写保留；落盘随同步卡）。
///
/// # Errors
/// 应用未初始化。
#[tauri::command]
pub async fn cmd_plugin_audit_logs(
    state: State<'_, PluginState>,
    limit: Option<usize>,
    plugin_id: Option<String>,
) -> Result<Vec<aurora_plugin::audit::AuditEntry>, String> {
    Ok(state
        .runtime
        .audit_recent(limit.unwrap_or(100).min(1024), plugin_id.as_deref()))
}

/// DK-15b 消费侧收口：插件输出拉取（拉取即消费——core UserOutputBuf drain
/// 语义透传）。UserOutput=产品向内容（print host API），与 log 诊断流分界
/// （15b 文档化）；无人拉取=64KiB FIFO 积压淘汰——本 cmd 是桌面轮询消费点。
///
/// # Errors
/// runtime host_call 失败（插件未注册等——字符串化透传 UI）。
#[tauri::command]
pub async fn cmd_pull_plugin_output(
    state: State<'_, PluginState>,
    plugin_id: String,
) -> Result<Vec<String>, String> {
    let out = state
        .runtime
        .call_host_function(
            &plugin_id,
            aurora_plugin::wasm_runtime::HostFunction::PullOutput,
            &serde_json::json!({}),
        )
        .map_err(|e| e.to_string())?;
    Ok(out
        .get("lines")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|l| l.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default())
}

// ===========================================================================
// DK-1.1 接线：桌面热升级命令面（统一注册表语义消费方——f84ba27 P0-2 的
// 命令层补全）。两段式对齐 liquify 先例：start（预装载→Pending 确认门）
// → confirm（落 KV+runtime 重载）。
//
// v1 边界（诚实化）：
// - blob 来源=wasm_path 文件（与 cmd_plugin_install 同模式——市场下载器
//   S2 落地后换字节流通道）；
// - runtime 重载=install_component 换字节+踢实例（下次 invoke 慢路径重
//   实例化）；capabilities 变更的重注册（L2 授权图谱连带）=升级卡 v2
//   边界——v1 manifest 不动（普通升级 capabilities 未变的常规路径安全）；
// - 落库失败=内存已切换 KV 滞后（f84ba27 最终一致边界延续）。
// ===========================================================================

use aurora_plugin::hot_update::{UpdateResult, UpdateState};

/// 热升级预装载（两段式第一段——统一注册表自动装载会话缓存→Pending）。
///
/// # Errors
/// 应用未初始化 / wasm 文件读失败 / 两域均无此件（NotFound）/ KV 失败。
#[tauri::command]
pub async fn cmd_plugin_hot_update_start(
    state: State<'_, PluginState>,
    plugin_id: String,
    new_version: String,
    wasm_path: String,
) -> Result<UpdateResult, String> {
    let booted = crate::sync_commands::get_booted()?;
    let new_blob = tokio::fs::read(&wasm_path)
        .await
        .map_err(|e| format!("read wasm `{wasm_path}`: {e}"))?;
    state
        .hot
        .preload_from_registry(
            booted.core.kv_store.as_ref(),
            plugin_id,
            new_version,
            new_blob,
        )
        .await
        .map_err(|e| e.to_string())
}

/// 热升级状态查询（确认门前刷新——Pending 才允许 confirm）。
///
/// # Errors
/// 应用未初始化。
#[tauri::command]
pub async fn cmd_plugin_hot_update_status(
    state: State<'_, PluginState>,
    plugin_id: String,
) -> Result<Option<UpdateState>, String> {
    Ok(state.hot.state(&plugin_id))
}

/// 热升级确认（两段式第二段——落 KV 统一注册表+runtime 重载换字节踢实例）。
///
/// # Errors
/// 应用未初始化 / 状态机拒绝（非 Switched）/ KV 失败 / blob 读失败。
#[tauri::command]
pub async fn cmd_plugin_hot_update_confirm(
    state: State<'_, PluginState>,
    plugin_id: String,
) -> Result<UpdateResult, String> {
    let booted = crate::sync_commands::get_booted()?;
    let result = state
        .hot
        .confirm_switch_persist(booted.core.kv_store.as_ref(), &plugin_id, false)
        .await
        .map_err(|e| e.to_string())?;
    // runtime 重载：新 blob 已落 KV（blob: 前缀）——读回换字节+踢实例
    //（下次 invoke 慢路径重新实例化；cached_components 同步踢除防旧
    // 编译件复用）。
    let new_bytes = booted
        .core
        .kv_store
        .get(&format!("{}{plugin_id}", installer::BLOB_PREFIX))
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("blob missing after persist: {plugin_id}"))?;
    state.runtime.install_component(&plugin_id, new_bytes);
    state.runtime.take_cached_component(&plugin_id);
    Ok(result)
}

/// 热升级回滚（Switched 后发现新版本问题——退回旧版本字节并落库）。
///
/// Pending 态放弃无需命令：不 confirm 即滞留 Pending（无副作用，下一次
/// preload 覆盖）。
///
/// # Errors
/// 应用未初始化 / 状态机拒绝（非 Switched）/ KV 失败。
#[tauri::command]
pub async fn cmd_plugin_hot_update_rollback(
    state: State<'_, PluginState>,
    plugin_id: String,
) -> Result<UpdateResult, String> {
    let booted = crate::sync_commands::get_booted()?;
    let result = state.hot.rollback(&plugin_id).map_err(|e| e.to_string())?;
    // 回滚后 runtime 同步换回旧字节（旧 blob=确认前落库值已还原）
    let old_bytes = booted
        .core
        .kv_store
        .get(&format!("{}{plugin_id}", installer::BLOB_PREFIX))
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("blob missing after rollback: {plugin_id}"))?;
    state.runtime.install_component(&plugin_id, old_bytes);
    state.runtime.take_cached_component(&plugin_id);
    Ok(result)
}
