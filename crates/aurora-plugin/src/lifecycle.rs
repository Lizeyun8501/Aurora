//! 插件生命周期与双模式架构 (Plugin Lifecycle & Dual-Mode Architecture)
//!
//! 支持 WASM 与 iframe 两种沙箱模式，统一管理插件生命周期状态机：
//! `Loaded → Initialized → Running ⇄ Suspended → Unloaded`。
//!
//! # 设计要点
//! - **双模式**：`PluginMode::Wasm` 提供强隔离与确定性执行；`PluginMode::Iframe`
//!   提供富 UI 渲染能力。`PluginManager` 按清单声明的运行时类型自动选择。
//! - **状态机**：通过 `PluginLifecycle` 强校验状态迁移，非法迁移返回
//!   `Error::InvalidStateTransition`。
//! - **统一调度**：`PluginManager` 实现 `aurora_core::traits::PluginRuntime`，
//!   对外暴露 `load / invoke / unload / list_hooks`，内部按模式分派到
//!   `WasmRuntime` 或 `IframeRuntime`。

use async_trait::async_trait;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use aurora_core::traits::plugin_runtime::{PluginHandle, PluginManifest, PluginRuntime, RuntimeType};

use crate::iframe_runtime::IframeRuntime;
use crate::permission::{Permission, PermissionEngine};
use crate::wasm_runtime::{CapabilityManifest, WasmRuntime};

/// 插件唯一标识。
pub type PluginId = String;

/// 插件运行模式。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum PluginMode {
    /// WASM 沙箱：强隔离、确定性、可计量
    Wasm,
    /// iframe 沙箱：富 UI、postMessage 通信
    Iframe,
}

impl From<RuntimeType> for PluginMode {
    fn from(rt: RuntimeType) -> Self {
        match rt {
            RuntimeType::Wasm => PluginMode::Wasm,
            RuntimeType::Iframe => PluginMode::Iframe,
        }
    }
}

impl PluginMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            PluginMode::Wasm => "wasm",
            PluginMode::Iframe => "iframe",
        }
    }
}

/// 插件运行状态。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum PluginStatus {
    /// 已加载（清单解析完成，资源就位）
    Loaded,
    /// 已初始化（沙箱已创建，宿主函数已绑定）
    Initialized,
    /// 运行中
    Running,
    /// 已挂起（暂停执行，保留状态）
    Suspended,
    /// 已卸载（资源释放）
    Unloaded,
    /// 错误状态
    Error,
}

impl PluginStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            PluginStatus::Loaded => "loaded",
            PluginStatus::Initialized => "initialized",
            PluginStatus::Running => "running",
            PluginStatus::Suspended => "suspended",
            PluginStatus::Unloaded => "unloaded",
            PluginStatus::Error => "error",
        }
    }
}

/// 插件生命周期状态机，校验合法的状态迁移。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PluginLifecycle {
    status: PluginStatus,
}

impl Default for PluginLifecycle {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginLifecycle {
    /// 创建初始状态为 `Loaded` 的生命周期。
    pub fn new() -> Self {
        Self {
            status: PluginStatus::Loaded,
        }
    }

    /// 当前状态。
    pub fn status(&self) -> PluginStatus {
        self.status
    }

    /// 尝试迁移到目标状态，非法迁移返回 `InvalidStateTransition`。
    pub fn transition(&mut self, to: PluginStatus) -> Result<(), crate::Error> {
        let from = self.status;
        let allowed = Self::is_valid_transition(from, to);
        if !allowed {
            warn!("invalid lifecycle transition: {:?} -> {:?}", from, to);
            return Err(crate::Error::InvalidStateTransition { from, to });
        }
        debug!("plugin lifecycle: {:?} -> {:?}", from, to);
        self.status = to;
        Ok(())
    }

    /// 判断状态迁移是否合法。
    fn is_valid_transition(from: PluginStatus, to: PluginStatus) -> bool {
        use PluginStatus::*;
        match (from, to) {
            (Loaded, Initialized)
            | (Initialized, Running)
            | (Running, Suspended)
            | (Suspended, Running)
            | (Loaded, Unloaded)
            | (Initialized, Unloaded)
            | (Running, Unloaded)
            | (Suspended, Unloaded)
            | (Error, Unloaded)
            | (Error, Loaded) => true,
            // 任意非卸载状态可进入 Error
            (s, Error) if s != Unloaded => true,
            (Unloaded, Unloaded) => true,
            _ => false,
        }
    }
}

/// 已注册的插件实例。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plugin {
    pub id: PluginId,
    pub manifest: PluginManifest,
    pub mode: PluginMode,
    pub status: PluginStatus,
    pub capabilities: CapabilityManifest,
    pub permissions: HashSet<Permission>,
    pub loaded_at: chrono::DateTime<chrono::Utc>,
}

impl Plugin {
    /// 根据清单与模式构造插件实例（初始状态 `Loaded`）。
    pub fn new(manifest: PluginManifest, mode: PluginMode) -> Self {
        let capabilities = CapabilityManifest::from_permissions(&manifest.permissions);
        let permissions = manifest
            .permissions
            .iter()
            .filter_map(|p| Permission::parse(p.as_str()))
            .collect();
        Self {
            id: manifest.id.clone(),
            manifest,
            mode,
            status: PluginStatus::Loaded,
            capabilities,
            permissions,
            loaded_at: chrono::Utc::now(),
        }
    }

    /// 是否处于可调用状态（Running）。
    pub fn is_running(&self) -> bool {
        self.status == PluginStatus::Running
    }
}

/// 构造一份用于测试的最小插件清单。
#[cfg(test)]
fn sample_manifest(id: &str, mode: RuntimeType, perms: &[&str]) -> PluginManifest {
    let entry = match mode {
        RuntimeType::Wasm => "plugin.wasm".to_string(),
        RuntimeType::Iframe => "plugin.html".to_string(),
    };
    PluginManifest {
        id: id.to_string(),
        name: format!("{} plugin", id),
        version: "1.0.0".to_string(),
        author: "aurora".to_string(),
        description: "sample".to_string(),
        runtime: mode,
        entry,
        permissions: perms.iter().map(|s| s.to_string()).collect(),
        hooks: vec!["on_save".to_string()],
        block_types: vec![],
        config_schema: None,
    }
}

/// 插件管理器：双模式调度 + 生命周期 + 权限的统一入口。
///
/// 实现 `aurora_core::traits::PluginRuntime`，作为宿主对插件系统的标准接口。
pub struct PluginManager {
    plugins: Arc<RwLock<HashMap<PluginId, Plugin>>>,
    /// 钩子点 → 已注册该钩子的插件 id 列表
    hooks: Arc<RwLock<HashMap<String, Vec<PluginId>>>>,
    wasm_runtime: Arc<WasmRuntime>,
    iframe_runtime: Arc<IframeRuntime>,
    permission_engine: Arc<PermissionEngine>,
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginManager {
    /// 使用默认 WASM / iframe / 权限运行时构造管理器。
    pub fn new() -> Self {
        Self {
            plugins: Arc::new(RwLock::new(HashMap::new())),
            hooks: Arc::new(RwLock::new(HashMap::new())),
            wasm_runtime: Arc::new(WasmRuntime::with_default_registry()),
            iframe_runtime: Arc::new(IframeRuntime::with_light_theme()),
            permission_engine: Arc::new(PermissionEngine::with_default_grantor()),
        }
    }

    /// 注入自定义子运行时（便于测试）。
    pub fn with_runtimes(
        wasm: Arc<WasmRuntime>,
        iframe: Arc<IframeRuntime>,
        permissions: Arc<PermissionEngine>,
    ) -> Self {
        Self {
            plugins: Arc::new(RwLock::new(HashMap::new())),
            hooks: Arc::new(RwLock::new(HashMap::new())),
            wasm_runtime: wasm,
            iframe_runtime: iframe,
            permission_engine: permissions,
        }
    }

    /// 注册插件实例并绑定沙箱。
    pub fn register_plugin(&self, plugin: Plugin) {
        let id = plugin.id.clone();
        let mode = plugin.mode;
        let caps = plugin.capabilities.clone();
        // 注册钩子
        for hook in &plugin.manifest.hooks {
            self.hooks
                .write()
                .entry(hook.clone())
                .or_default()
                .push(id.clone());
        }
        // 按模式绑定到对应沙箱
        match mode {
            PluginMode::Wasm => self.wasm_runtime.register(id.clone(), caps),
            PluginMode::Iframe => {
                self.iframe_runtime.register(id.clone(), &plugin.manifest.entry);
            }
        }
        // 预授予清单声明的权限
        for perm in &plugin.permissions {
            let _ = self.permission_engine.grant(&id, perm.clone(), Some("manifest".into()));
        }
        info!("plugin registered: {} ({:?})", id, mode);
        self.plugins.write().insert(id, plugin);
    }

    /// 返回插件实例快照。
    pub fn get_plugin(&self, id: &str) -> Option<Plugin> {
        self.plugins.read().get(id).cloned()
    }

    /// 返回当前状态。
    pub fn status(&self, id: &str) -> Option<PluginStatus> {
        self.plugins.read().get(id).map(|p| p.status)
    }

    /// 列出全部已注册插件。
    pub fn list(&self) -> Vec<Plugin> {
        self.plugins.read().values().cloned().collect()
    }

    /// 初始化：`Loaded → Initialized`。
    pub fn init(&self, id: &str) -> Result<(), crate::Error> {
        let mut plugins = self.plugins.write();
        let plugin = plugins
            .get_mut(id)
            .ok_or_else(|| crate::Error::NotFound(format!("plugin not found: {}", id)))?;
        match plugin.status {
            PluginStatus::Loaded => {
                plugin.status = PluginStatus::Initialized;
                debug!("plugin {} initialized", id);
                Ok(())
            }
            other => Err(crate::Error::InvalidStateTransition {
                from: other,
                to: PluginStatus::Initialized,
            }),
        }
    }

    /// 启动：`Initialized → Running`。
    pub fn start(&self, id: &str) -> Result<(), crate::Error> {
        let mut plugins = self.plugins.write();
        let plugin = plugins
            .get_mut(id)
            .ok_or_else(|| crate::Error::NotFound(format!("plugin not found: {}", id)))?;
        match plugin.status {
            PluginStatus::Initialized => {
                plugin.status = PluginStatus::Running;
                info!("plugin {} running", id);
                Ok(())
            }
            other => Err(crate::Error::InvalidStateTransition {
                from: other,
                to: PluginStatus::Running,
            }),
        }
    }

    /// 挂起：`Running → Suspended`。
    pub fn suspend(&self, id: &str) -> Result<(), crate::Error> {
        let mut plugins = self.plugins.write();
        let plugin = plugins
            .get_mut(id)
            .ok_or_else(|| crate::Error::NotFound(format!("plugin not found: {}", id)))?;
        match plugin.status {
            PluginStatus::Running => {
                plugin.status = PluginStatus::Suspended;
                debug!("plugin {} suspended", id);
                Ok(())
            }
            other => Err(crate::Error::InvalidStateTransition {
                from: other,
                to: PluginStatus::Suspended,
            }),
        }
    }

    /// 恢复：`Suspended → Running`。
    pub fn resume(&self, id: &str) -> Result<(), crate::Error> {
        let mut plugins = self.plugins.write();
        let plugin = plugins
            .get_mut(id)
            .ok_or_else(|| crate::Error::NotFound(format!("plugin not found: {}", id)))?;
        match plugin.status {
            PluginStatus::Suspended => {
                plugin.status = PluginStatus::Running;
                debug!("plugin {} resumed", id);
                Ok(())
            }
            other => Err(crate::Error::InvalidStateTransition {
                from: other,
                to: PluginStatus::Running,
            }),
        }
    }

    /// 卸载插件：迁移到 `Unloaded` 并从沙箱注销。
    pub fn unload_plugin(&self, id: &str) -> Result<(), crate::Error> {
        let mut plugins = self.plugins.write();
        let plugin = plugins
            .get_mut(id)
            .ok_or_else(|| crate::Error::NotFound(format!("plugin not found: {}", id)))?;
        if plugin.status == PluginStatus::Unloaded {
            return Ok(());
        }
        let from = plugin.status;
        if !PluginLifecycle::is_valid_transition(from, PluginStatus::Unloaded) {
            return Err(crate::Error::InvalidStateTransition {
                from,
                to: PluginStatus::Unloaded,
            });
        }
        let mode = plugin.mode;
        plugin.status = PluginStatus::Unloaded;
        drop(plugins);
        match mode {
            PluginMode::Wasm => self.wasm_runtime.unregister(id),
            PluginMode::Iframe => self.iframe_runtime.unregister(id),
        }
        // 从钩子注册表中移除
        let mut hooks = self.hooks.write();
        for ids in hooks.values_mut() {
            ids.retain(|h| h != id);
        }
        info!("plugin {} unloaded", id);
        Ok(())
    }

    /// 按模式分派调用。
    pub fn invoke_plugin(
        &self,
        id: &str,
        method: &str,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, crate::Error> {
        let plugin = self
            .get_plugin(id)
            .ok_or_else(|| crate::Error::NotFound(format!("plugin not found: {}", id)))?;
        if !plugin.is_running() {
            return Err(crate::Error::InvalidInput(format!(
                "plugin {} is not running (state={:?})",
                id, plugin.status
            )));
        }
        match plugin.mode {
            PluginMode::Wasm => self.wasm_runtime.invoke(id, method, args),
            PluginMode::Iframe => self.iframe_runtime.invoke(id, method, args),
        }
    }

    /// 检查插件是否拥有指定权限。
    pub fn check_permission(&self, id: &str, perm: &Permission) -> bool {
        self.permission_engine.check(id, perm)
    }

    /// 返回某钩子点下已注册的插件 id 列表。
    pub fn hook_plugin_ids(&self, hook_point: &str) -> Vec<PluginId> {
        self.hooks
            .read()
            .get(hook_point)
            .cloned()
            .unwrap_or_default()
    }

    /// 返回 WASM 运行时引用。
    pub fn wasm_runtime(&self) -> &WasmRuntime {
        &self.wasm_runtime
    }

    /// 返回 iframe 运行时引用。
    pub fn iframe_runtime(&self) -> &IframeRuntime {
        &self.iframe_runtime
    }

    /// 返回权限引擎引用。
    pub fn permission_engine(&self) -> &PermissionEngine {
        &self.permission_engine
    }
}

#[async_trait]
impl PluginRuntime for PluginManager {
    async fn load(&mut self, manifest: &PluginManifest) -> Result<PluginHandle, aurora_core::Error> {
        let mode = PluginMode::from(manifest.runtime.clone());
        let plugin = Plugin::new(manifest.clone(), mode);
        let handle = PluginHandle {
            id: plugin.id.clone(),
            manifest: plugin.manifest.clone(),
        };
        self.register_plugin(plugin);
        Ok(handle)
    }

    async fn invoke(
        &self,
        handle: &PluginHandle,
        method: &str,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, aurora_core::Error> {
        self.invoke_plugin(&handle.id, method, args)
            .map_err(aurora_core::Error::from)
    }

    async fn unload(&mut self, handle: &PluginHandle) -> Result<(), aurora_core::Error> {
        self.unload_plugin(&handle.id).map_err(aurora_core::Error::from)
    }

    fn list_hooks(&self, hook_point: &str) -> Vec<PluginHandle> {
        let ids = self.hook_plugin_ids(hook_point);
        let plugins = self.plugins.read();
        ids.iter()
            .filter_map(|id| plugins.get(id).map(|p| PluginHandle {
                id: p.id.clone(),
                manifest: p.manifest.clone(),
            }))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wasm_runtime::Capability;

    #[test]
    fn test_plugin_mode_from_runtime_type() {
        assert_eq!(PluginMode::from(RuntimeType::Wasm), PluginMode::Wasm);
        assert_eq!(PluginMode::from(RuntimeType::Iframe), PluginMode::Iframe);
        assert_eq!(PluginMode::Wasm.as_str(), "wasm");
        assert_eq!(PluginMode::Iframe.as_str(), "iframe");
    }

    #[test]
    fn test_plugin_status_as_str() {
        assert_eq!(PluginStatus::Loaded.as_str(), "loaded");
        assert_eq!(PluginStatus::Running.as_str(), "running");
        assert_eq!(PluginStatus::Unloaded.as_str(), "unloaded");
    }

    #[test]
    fn test_lifecycle_initial_status() {
        let lc = PluginLifecycle::new();
        assert_eq!(lc.status(), PluginStatus::Loaded);
    }

    #[test]
    fn test_lifecycle_valid_transitions() {
        let mut lc = PluginLifecycle::new();
        lc.transition(PluginStatus::Initialized).unwrap();
        assert_eq!(lc.status(), PluginStatus::Initialized);
        lc.transition(PluginStatus::Running).unwrap();
        assert_eq!(lc.status(), PluginStatus::Running);
        lc.transition(PluginStatus::Suspended).unwrap();
        lc.transition(PluginStatus::Running).unwrap();
        lc.transition(PluginStatus::Unloaded).unwrap();
        assert_eq!(lc.status(), PluginStatus::Unloaded);
    }

    #[test]
    fn test_lifecycle_invalid_transition() {
        let mut lc = PluginLifecycle::new();
        // Loaded -> Running 非法（需先 Initialized）
        let err = lc.transition(PluginStatus::Running).unwrap_err();
        assert!(matches!(
            err,
            crate::Error::InvalidStateTransition {
                from: PluginStatus::Loaded,
                to: PluginStatus::Running
            }
        ));
        // 状态未改变
        assert_eq!(lc.status(), PluginStatus::Loaded);
    }

    #[test]
    fn test_lifecycle_unload_from_states() {
        for from in [
            PluginStatus::Loaded,
            PluginStatus::Initialized,
            PluginStatus::Running,
            PluginStatus::Suspended,
        ] {
            let mut lc = PluginLifecycle::new();
            // 把状态推到 from
            match from {
                PluginStatus::Loaded => {}
                PluginStatus::Initialized => {
                    lc.transition(PluginStatus::Initialized).unwrap();
                }
                PluginStatus::Running => {
                    lc.transition(PluginStatus::Initialized).unwrap();
                    lc.transition(PluginStatus::Running).unwrap();
                }
                PluginStatus::Suspended => {
                    lc.transition(PluginStatus::Initialized).unwrap();
                    lc.transition(PluginStatus::Running).unwrap();
                    lc.transition(PluginStatus::Suspended).unwrap();
                }
                _ => unreachable!(),
            }
            assert!(lc.transition(PluginStatus::Unloaded).is_ok());
        }
    }

    #[test]
    fn test_lifecycle_error_recovery() {
        let mut lc = PluginLifecycle::new();
        lc.transition(PluginStatus::Error).unwrap();
        assert_eq!(lc.status(), PluginStatus::Error);
        // Error -> Unloaded 合法
        lc.transition(PluginStatus::Unloaded).unwrap();
    }

    #[test]
    fn test_lifecycle_unloaded_cannot_resume() {
        let mut lc = PluginLifecycle::new();
        lc.transition(PluginStatus::Initialized).unwrap();
        lc.transition(PluginStatus::Running).unwrap();
        lc.transition(PluginStatus::Unloaded).unwrap();
        // Unloaded -> Running 非法
        assert!(lc.transition(PluginStatus::Running).is_err());
    }

    #[test]
    fn test_plugin_new_derives_capabilities_and_permissions() {
        let manifest = sample_manifest("p1", RuntimeType::Wasm, &["fs", "storage", "clipboard"]);
        let plugin = Plugin::new(manifest, PluginMode::Wasm);
        assert_eq!(plugin.id, "p1");
        assert_eq!(plugin.mode, PluginMode::Wasm);
        assert_eq!(plugin.status, PluginStatus::Loaded);
        assert!(plugin.capabilities.grants(Capability::Fs));
        assert!(plugin.capabilities.grants(Capability::Storage));
        assert!(!plugin.capabilities.grants(Capability::Network));
        assert!(plugin.permissions.contains(&Permission::Fs));
        assert!(plugin.permissions.contains(&Permission::Storage));
        assert!(plugin.permissions.contains(&Permission::Clipboard));
        assert!(!plugin.is_running());
    }

    #[test]
    fn test_plugin_manager_load_and_status() {
        let mut mgr = PluginManager::new();
        let manifest = sample_manifest("p1", RuntimeType::Wasm, &["storage"]);
        let handle = mgr.load(&manifest).unwrap();
        assert_eq!(handle.id, "p1");
        assert_eq!(mgr.status("p1"), Some(PluginStatus::Loaded));
        assert_eq!(mgr.list().len(), 1);
        // 权限应被预授予
        assert!(mgr.check_permission("p1", &Permission::Storage));
    }

    #[test]
    fn test_plugin_manager_lifecycle_init_start_suspend_resume() {
        let mgr = PluginManager::new();
        let manifest = sample_manifest("p1", RuntimeType::Wasm, &[]);
        mgr.register_plugin(Plugin::new(manifest, PluginMode::Wasm));

        assert_eq!(mgr.status("p1").unwrap(), PluginStatus::Loaded);
        // 未初始化直接 start 应失败
        assert!(mgr.start("p1").is_err());
        mgr.init("p1").unwrap();
        assert_eq!(mgr.status("p1").unwrap(), PluginStatus::Initialized);
        mgr.start("p1").unwrap();
        assert_eq!(mgr.status("p1").unwrap(), PluginStatus::Running);
        mgr.suspend("p1").unwrap();
        assert_eq!(mgr.status("p1").unwrap(), PluginStatus::Suspended);
        mgr.resume("p1").unwrap();
        assert_eq!(mgr.status("p1").unwrap(), PluginStatus::Running);
    }

    #[test]
    fn test_plugin_manager_invoke_wasm_dispatch() {
        let mgr = PluginManager::new();
        let manifest = sample_manifest("p1", RuntimeType::Wasm, &["storage"]);
        mgr.register_plugin(Plugin::new(manifest, PluginMode::Wasm));
        mgr.init("p1").unwrap();
        mgr.start("p1").unwrap();

        // 调用宿主函数 write_doc（需 Storage，已授予）
        let out = mgr
            .invoke_plugin("p1", "write_doc", &serde_json::json!({"doc_id": "d", "data": 1}))
            .unwrap();
        assert_eq!(out["ok"], serde_json::json!(true));
        let read = mgr
            .invoke_plugin("p1", "read_doc", &serde_json::json!({"doc_id": "d"}))
            .unwrap();
        assert_eq!(read, serde_json::json!(1));
    }

    #[test]
    fn test_plugin_manager_invoke_iframe_dispatch() {
        let mgr = PluginManager::new();
        let manifest = sample_manifest("p1", RuntimeType::Iframe, &[]);
        mgr.register_plugin(Plugin::new(manifest, PluginMode::Iframe));
        mgr.init("p1").unwrap();
        mgr.start("p1").unwrap();
        let out = mgr.invoke_plugin("p1", "ping", &serde_json::json!({})).unwrap();
        assert_eq!(out, serde_json::json!("pong"));
    }

    #[test]
    fn test_plugin_manager_invoke_not_running_rejected() {
        let mgr = PluginManager::new();
        let manifest = sample_manifest("p1", RuntimeType::Wasm, &[]);
        mgr.register_plugin(Plugin::new(manifest, PluginMode::Wasm));
        // Loaded 状态调用应失败
        let err = mgr.invoke_plugin("p1", "ping", &serde_json::json!({})).unwrap_err();
        assert!(matches!(err, crate::Error::InvalidInput(_)));
    }

    #[test]
    fn test_plugin_manager_unload() {
        let mgr = PluginManager::new();
        let manifest = sample_manifest("p1", RuntimeType::Wasm, &[]);
        mgr.register_plugin(Plugin::new(manifest, PluginMode::Wasm));
        mgr.init("p1").unwrap();
        mgr.start("p1").unwrap();
        mgr.unload_plugin("p1").unwrap();
        assert_eq!(mgr.status("p1").unwrap(), PluginStatus::Unloaded);
        // 卸载后调用应失败（not running）
        assert!(mgr.invoke_plugin("p1", "ping", &serde_json::json!({})).is_err());
    }

    #[test]
    fn test_plugin_manager_list_hooks() {
        let mgr = PluginManager::new();
        let m1 = sample_manifest("p1", RuntimeType::Wasm, &[]);
        let mut m2 = sample_manifest("p2", RuntimeType::Wasm, &[]);
        m2.hooks = vec!["on_save".to_string(), "on_open".to_string()];
        mgr.register_plugin(Plugin::new(m1, PluginMode::Wasm));
        mgr.register_plugin(Plugin::new(m2, PluginMode::Wasm));

        let on_save = mgr.hook_plugin_ids("on_save");
        // m1 和 m2 都注册了 on_save
        assert_eq!(on_save.len(), 2);
        let on_open = mgr.hook_plugin_ids("on_open");
        assert_eq!(on_open.len(), 1);
        assert_eq!(on_open[0], "p2");
        let handles = mgr.list_hooks("on_save");
        assert_eq!(handles.len(), 2);
    }

    #[test]
    fn test_plugin_manager_unload_removes_hooks() {
        let mgr = PluginManager::new();
        let manifest = sample_manifest("p1", RuntimeType::Wasm, &[]);
        mgr.register_plugin(Plugin::new(manifest, PluginMode::Wasm));
        assert_eq!(mgr.hook_plugin_ids("on_save").len(), 1);
        mgr.unload_plugin("p1").unwrap();
        assert_eq!(mgr.hook_plugin_ids("on_save").len(), 0);
    }

    #[test]
    fn test_plugin_runtime_trait_round_trip() {
        let mut mgr = PluginManager::new();
        let manifest = sample_manifest("p1", RuntimeType::Iframe, &[]);
        let handle = mgr.load(&manifest).unwrap();
        // trait 方法初始化需经 manager 自身（trait 仅暴露 load/invoke/unload）
        mgr.init(&handle.id).unwrap();
        mgr.start(&handle.id).unwrap();
        let out = mgr.invoke(&handle, "ping", &serde_json::json!({})).unwrap();
        assert_eq!(out, serde_json::json!("pong"));
        mgr.unload(&handle).unwrap();
        assert_eq!(mgr.status(&handle.id).unwrap(), PluginStatus::Unloaded);
    }

    #[test]
    fn test_plugin_manager_unknown_plugin_errors() {
        let mgr = PluginManager::new();
        assert!(mgr.init("ghost").is_err());
        assert!(mgr.start("ghost").is_err());
        assert!(mgr.unload_plugin("ghost").is_err());
        assert!(mgr.status("ghost").is_none());
    }
}
