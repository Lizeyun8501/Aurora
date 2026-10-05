//! WASM 运行时 (基于 Wasmtime)
//!
//! 提供安全沙箱化的 WebAssembly 运行时能力，用于隔离执行插件代码。
//! 底层使用 [Wasmtime](https://wasmtime.dev/) 实现。

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Mutex;

use crate::traits::plugin_runtime::{PluginHandle, PluginManifest, PluginRuntime, RuntimeType};

/// 基于 Wasmtime 的 WASM 运行时实现。
pub struct WasmtimeRuntime {
    // V26 DK-14 插件生态迭代使用（多模块共享同一 Engine）
    #[allow(dead_code)]
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

#[async_trait]
impl PluginRuntime for WasmtimeRuntime {
    async fn load(&mut self, manifest: &PluginManifest) -> Result<PluginHandle, crate::Error> {
        if manifest.runtime != RuntimeType::Wasm {
            return Err(crate::Error::InvalidInput(format!(
                "WasmtimeRuntime expects Wasm runtime, got {:?}",
                manifest.runtime
            )));
        }
        // DK-33: manifest.entry 字节码读取 → wasmtime 编译（复用 DK-31 后的
        // 共享 Engine 实例，不新增配置面）。
        let bytes = std::fs::read(&manifest.entry).map_err(|e| {
            crate::Error::Internal(format!(
                "wasm plugin entry read failed: {} ({e})",
                manifest.entry
            ))
        })?;
        let module = wasmtime::Module::new(&self.engine, &bytes)
            .map_err(|e| crate::Error::Internal(format!("wasm compile: {e}")))?;
        tracing::info!(
            "wasmtime load plugin: id={}, entry={}",
            manifest.id,
            manifest.entry
        );
        let handle = PluginHandle {
            id: manifest.id.clone(),
            manifest: manifest.clone(),
        };
        let mut modules = self
            .modules
            .lock()
            .map_err(|_| crate::Error::Internal("wasmtime modules mutex poisoned".to_string()))?;
        modules.insert(manifest.id.clone(), module);
        Ok(handle)
    }

    async fn invoke(
        &self,
        handle: &PluginHandle,
        method: &str,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, crate::Error> {
        let module = {
            let modules = self.modules.lock().map_err(|_| {
                crate::Error::Internal("wasmtime modules mutex poisoned".to_string())
            })?;
            modules
                .get(&handle.id)
                .cloned()
                .ok_or_else(|| crate::Error::NoteNotFound {
                    id: format!("plugin:{}", handle.id),
                })?
        };
        tracing::info!(
            "wasmtime invoke: plugin={}, method={}, args={}",
            handle.id,
            method,
            args
        );
        // DK-33: TypedFunc 调用链——最小契约两形态：
        //   args 为 number → `(i32) -> i32`；否则 → `() -> i32`。
        // （&str/String 需 guest 内存编解码，属 host-fn/wit 面——后续卡）
        // per-call 新建 Store+Instance：无 host 状态，幂等插件语义最小正确；
        // 有状态插件（Store 持久缓存）后续卡扩展。
        let mut store = wasmtime::Store::new(&self.engine, ());
        let linker = wasmtime::Linker::<()>::new(&self.engine);
        let instance = linker
            .instantiate(&mut store, &module)
            .map_err(|e| crate::Error::Internal(format!("wasm instantiate: {e}")))?;
        let value = match args.as_i64() {
            Some(v) => {
                let f = instance
                    .get_typed_func::<i32, i32>(&mut store, method)
                    .map_err(|e| crate::Error::Internal(format!("wasm export (i32)->i32: {e}")))?;
                let r = f
                    .call(&mut store, v.clamp(i32::MIN as i64, i32::MAX as i64) as i32)
                    .map_err(|e| crate::Error::Internal(format!("wasm call: {e}")))?;
                serde_json::Value::from(r)
            }
            None => {
                let f = instance
                    .get_typed_func::<(), i32>(&mut store, method)
                    .map_err(|e| crate::Error::Internal(format!("wasm export ()->i32: {e}")))?;
                let r = f
                    .call(&mut store, ())
                    .map_err(|e| crate::Error::Internal(format!("wasm call: {e}")))?;
                serde_json::Value::from(r)
            }
        };
        Ok(value)
    }

    async fn unload(&mut self, handle: &PluginHandle) -> Result<(), crate::Error> {
        let mut modules = self
            .modules
            .lock()
            .map_err(|_| crate::Error::Internal("wasmtime modules mutex poisoned".to_string()))?;
        modules.remove(&handle.id);
        let mut stores = self
            .stores
            .lock()
            .map_err(|_| crate::Error::Internal("wasmtime stores mutex poisoned".to_string()))?;
        stores.remove(&handle.id);
        tracing::info!("wasmtime unload: plugin={}", handle.id);
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

#[async_trait]
impl PluginRuntime for IframeRuntime {
    async fn load(&mut self, manifest: &PluginManifest) -> Result<PluginHandle, crate::Error> {
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

    async fn invoke(
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

    async fn unload(&mut self, handle: &PluginHandle) -> Result<(), crate::Error> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::plugin_runtime::{PluginManifest, RuntimeType};

    /// e2e 样本（wat 内联编译——DK-33 DoD：真实 wasm 字节码，不引入工具链）：
    /// 导出 `answer() -> i32`（常量 42）与 `bump(i32) -> i32`（+1）。
    fn write_sample_wasm(dir: &std::path::Path, name: &str) -> String {
        let wasm = wat::parse_str(
            r#"
            (module
                (func (export "answer") (result i32) (i32.const 42))
                (func (export "bump") (param i32) (result i32)
                    (i32.add (local.get 0) (i32.const 1)))
            )
        "#,
        )
        .expect("wat parse");
        let path = dir.join(name);
        std::fs::write(&path, wasm).expect("write wasm");
        path.to_string_lossy().to_string()
    }

    fn manifest(entry: String) -> PluginManifest {
        PluginManifest {
            id: "test-plugin".into(),
            name: "测试插件".into(),
            version: "0.1.0".into(),
            author: "bravo".into(),
            description: "e2e sample".into(),
            runtime: RuntimeType::Wasm,
            entry,
            permissions: vec![],
            hooks: vec![],
            block_types: vec![],
            config_schema: None,
        }
    }

    /// DK-33 DoD: 加载（entry 字节码 → Module 编译）+ invoke（TypedFunc 调用）。
    #[tokio::test]
    async fn dk33_load_and_invoke_e2e() {
        let dir = tempfile::tempdir().unwrap();
        let entry = write_sample_wasm(dir.path(), "plugin.wasm");
        let mut rt = WasmtimeRuntime::new().unwrap();
        let handle = rt.load(&manifest(entry)).await.unwrap();

        // () -> i32
        let out = rt
            .invoke(&handle, "answer", &serde_json::Value::Null)
            .await
            .unwrap();
        assert_eq!(out, serde_json::json!(42));
        // (i32) -> i32
        let out = rt
            .invoke(&handle, "bump", &serde_json::json!(41))
            .await
            .unwrap();
        assert_eq!(out, serde_json::json!(42));
    }

    /// unload 后 invoke 报错（模块已清理）；entry 缺失 load 报错。
    #[tokio::test]
    async fn dk33_unload_and_missing_entry() {
        let dir = tempfile::tempdir().unwrap();
        let entry = write_sample_wasm(dir.path(), "plugin.wasm");
        let mut rt = WasmtimeRuntime::new().unwrap();
        let handle = rt.load(&manifest(entry.clone())).await.unwrap();
        rt.unload(&handle).await.unwrap();
        assert!(rt
            .invoke(&handle, "answer", &serde_json::Value::Null)
            .await
            .is_err());

        let missing = dir.path().join("absent.wasm").to_string_lossy().to_string();
        assert!(
            rt.load(&manifest(missing)).await.is_err(),
            "entry 缺失须报错"
        );
        let _ = entry;
    }
}
