//! WASM 运行时 (基于 Wasmtime)
//!
//! 提供安全沙箱化的 WebAssembly 运行时能力，用于隔离执行插件代码。
//! 底层使用 [Wasmtime](https://wasmtime.dev/) 实现。

use std::collections::HashMap;
use std::sync::Mutex;

use crate::traits::plugin_runtime::{PluginHandle, PluginManifest, PluginRuntime, RuntimeType};

/// 基于 Wasmtime 的 WASM 运行时实现。
pub struct WasmtimeRuntime {
    engine: wasmtime::Engine,
    modules: Mutex<HashMap<String, wasmtime::Module>>,
    stores: Mutex<HashMap<String, wasmtime::Store<()>>>,
}

impl WasmtimeRuntime {
    /// 创建新的 Wasmtime 运行时实例。
    pub fn new() -> Result<Self, crate::Error> {
        let engine = wasmtime::Engine::default();
        Ok(Self {
            engine,
            modules: Mutex::new(HashMap::new()),
            stores: Mutex::new(HashMap::new()),
        })
    }
}

impl Default for WasmtimeRuntime {
    fn default() -> Self {
        Self::new().expect("wasmtime engine creation should not fail")
    }
}

impl PluginRuntime for WasmtimeRuntime {
    fn load(&mut self, manifest: &PluginManifest) -> Result<PluginHandle, crate::Error> {
        if manifest.runtime != RuntimeType::Wasm {
            return Err(crate::Error::InvalidInput(format!(
                "WasmtimeRuntime expects Wasm runtime, got {:?}",
                manifest.runtime
            )));
        }
        // TODO: 从 manifest.entry 路径读取 WASM 字节码并编译为 Module。
        tracing::info!("wasmtime load plugin: id={}, entry={}", manifest.id, manifest.entry);
        let handle = PluginHandle {
            id: manifest.id.clone(),
            manifest: manifest.clone(),
        };
        let mut modules = self
            .modules
            .lock()
            .map_err(|_| crate::Error::Internal("wasmtime modules mutex poisoned".to_string()))?;
        // 占位：注册一个空模块，真实实现需编译 WASM 文件。
        // modules.insert(manifest.id.clone(), module);
        Ok(handle)
    }

    fn invoke(
        &self,
        handle: &PluginHandle,
        method: &str,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, crate::Error> {
        let _modules = self
            .modules
            .lock()
            .map_err(|_| crate::Error::Internal("wasmtime modules mutex poisoned".to_string()))?;
        tracing::info!(
            "wasmtime invoke: plugin={}, method={}, args={}",
            handle.id,
            method,
            args
        );
        // TODO: 通过 wasmtime 的 TypedFunc 调用导出函数。
        Ok(serde_json::Value::Null)
    }

    fn unload(&mut self, handle: &PluginHandle) -> Result<(), crate::Error> {
        let mut modules = self
            .modules
            .lock()
            .map_err(|_| crate::Error::Internal("wasmtime modules mutex poisoned".to_string()))?;
        let mut stores = self
            .stores
            .lock()
            .map_err(|_| crate::Error::Internal("wasmtime stores mutex poisoned".to_string()))?;
        modules.remove(&handle.id);
        stores.remove(&handle.id);
        Ok(())
    }

    fn list_hooks(&self, hook_point: &str) -> Vec<PluginHandle> {
        tracing::debug!("wasmtime list_hooks: hook_point={}", hook_point);
        vec![]
    }
}

/// 基于 iframe 的插件运行时实现（主要用于 Web / Electron 前端）。
///
/// 在当前 native 核心层中，IframeRuntime 仅作为接口占位，
/// 真实执行由前端环境通过 JS bridge 完成。
pub struct IframeRuntime {
    plugins: Mutex<HashMap<String, PluginHandle>>,
}

impl IframeRuntime {
    /// 创建新的 Iframe 运行时实例。
    pub fn new() -> Self {
        Self {
            plugins: Mutex::new(HashMap::new()),
        }
    }
}

impl Default for IframeRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginRuntime for IframeRuntime {
    fn load(&mut self, manifest: &PluginManifest) -> Result<PluginHandle, crate::Error> {
        if manifest.runtime != RuntimeType::Iframe {
            return Err(crate::Error::InvalidInput(format!(
                "IframeRuntime expects Iframe runtime, got {:?}",
                manifest.runtime
            )));
        }
        let handle = PluginHandle {
            id: manifest.id.clone(),
            manifest: manifest.clone(),
        };
        let mut plugins = self
            .plugins
            .lock()
            .map_err(|_| crate::Error::Internal("iframe plugins mutex poisoned".to_string()))?;
        plugins.insert(handle.id.clone(), handle.clone());
        Ok(handle)
    }

    fn invoke(
        &self,
        handle: &PluginHandle,
        method: &str,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, crate::Error> {
        tracing::info!(
            "iframe invoke: plugin={}, method={}, args={}",
            handle.id,
            method,
            args
        );
        // 在 native 层中，iframe 调用需通过外部 JS bridge 转发。
        Err(crate::Error::Internal(
            "IframeRuntime::invoke requires JS bridge in web environment".to_string(),
        ))
    }

    fn unload(&mut self, handle: &PluginHandle) -> Result<(), crate::Error> {
        let mut plugins = self
            .plugins
            .lock()
            .map_err(|_| crate::Error::Internal("iframe plugins mutex poisoned".to_string()))?;
        plugins.remove(&handle.id);
        Ok(())
    }

    fn list_hooks(&self, hook_point: &str) -> Vec<PluginHandle> {
        tracing::debug!("iframe list_hooks: hook_point={}", hook_point);
        vec![]
    }
}
