//! WASM 插件运行时 (WASM Plugin Runtime)
//!
//! 基于 Wasmtime + WASI preview1 的 WASM 沙箱运行时。本模块实现为
//! 注册表/调度器：校验能力清单（CapabilityManifest），并通过
//! `HostFunctionRegistry` trait 分派宿主函数调用。
//!
//! # 生产环境接线说明
//! 真正的 Wasmtime 模块实例化（`Engine` + `Module` + `Linker` + WASI preview1）
//! 在生产环境中接入：从 `entry` 加载 `.wasm` 字节码、绑定宿主函数到 `Linker`、
//! 实例化并调用导出函数。本实现保留 `wasmtime::Engine` 句柄以表达运行时已就绪，
//! 核心调度逻辑（能力校验 + 宿主函数分派）在此完整实现并可独立测试。
//!
//! # 宿主函数
//! 暴露给 WASM 插件的宿主函数集合：`read_doc` / `write_doc` / `query` / `log`。
//! 每个宿主函数声明其所需能力（Capability），调用前先由 `CapabilityManifest` 校验。

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

/// 插件唯一标识（与 `crate::lifecycle::PluginId` 同型）
pub type PluginId = String;

/// WASM 沙箱能力粒度。最小权限原则下，插件只能调用其清单声明的能力。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Capability {
    /// 文件系统访问
    Fs,
    /// 网络访问
    Network,
    /// 原生宿主调用（剪贴板、通知、OS 集成等）
    Native,
    /// 文档/KV 存储访问
    Storage,
}

impl Capability {
    /// 返回能力的字符串标识
    pub fn as_str(&self) -> &'static str {
        match self {
            Capability::Fs => "fs",
            Capability::Network => "network",
            Capability::Native => "native",
            Capability::Storage => "storage",
        }
    }

    /// 从字符串解析能力
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "fs" => Some(Capability::Fs),
            "network" => Some(Capability::Network),
            "native" => Some(Capability::Native),
            "storage" => Some(Capability::Storage),
            _ => None,
        }
    }
}

/// 能力清单：声明某插件被授予的全部能力集合。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CapabilityManifest {
    pub capabilities: HashSet<Capability>,
}

impl CapabilityManifest {
    pub fn new() -> Self {
        Self::default()
    }

    /// 从权限字符串列表派生能力清单。
    /// 识别 `fs` / `network` / `native` / `storage`，忽略未知项。
    pub fn from_permissions(perms: &[String]) -> Self {
        let mut caps = HashSet::new();
        for p in perms {
            if let Some(cap) = Capability::parse(p.as_str()) {
                caps.insert(cap);
            } else if let Some(perm) = crate::permission::Permission::parse(p.as_str()) {
                if let Some(cap) = perm.as_capability() {
                    caps.insert(cap);
                }
            }
        }
        Self { capabilities: caps }
    }

    /// 追加一个能力
    pub fn grant(&mut self, cap: Capability) {
        self.capabilities.insert(cap);
    }

    /// 是否授予了指定能力
    pub fn grants(&self, cap: Capability) -> bool {
        self.capabilities.contains(&cap)
    }

    /// 要求某能力，未授予则返回错误
    pub fn require(&self, cap: Capability) -> Result<(), crate::Error> {
        if self.grants(cap) {
            Ok(())
        } else {
            Err(crate::Error::CapabilityDenied(cap))
        }
    }

    /// 已授予能力数量
    pub fn len(&self) -> usize {
        self.capabilities.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.capabilities.is_empty()
    }
}

/// 宿主函数枚举：暴露给 WASM 插件的宿主侧函数集合。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum HostFunction {
    /// 读取文档
    ReadDoc,
    /// 写入文档
    WriteDoc,
    /// 查询（搜索/检索）
    Query,
    /// 日志输出
    Log,
}

impl HostFunction {
    /// 返回宿主函数名（与 WASM import 名一致）
    pub fn name(&self) -> &'static str {
        match self {
            HostFunction::ReadDoc => "read_doc",
            HostFunction::WriteDoc => "write_doc",
            HostFunction::Query => "query",
            HostFunction::Log => "log",
        }
    }

    /// 从名称解析宿主函数
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "read_doc" => Some(HostFunction::ReadDoc),
            "write_doc" => Some(HostFunction::WriteDoc),
            "query" => Some(HostFunction::Query),
            "log" => Some(HostFunction::Log),
            _ => None,
        }
    }

    /// 调用该宿主函数所需的能力。`Log` 无需任何能力（始终允许）。
    pub fn required_capability(&self) -> Option<Capability> {
        match self {
            HostFunction::ReadDoc | HostFunction::WriteDoc | HostFunction::Query => {
                Some(Capability::Storage)
            }
            HostFunction::Log => None,
        }
    }
}

/// 宿主函数注册表 trait：定义宿主函数的具体实现来源。
///
/// 生产环境中由 `Linker` 绑定的 WASM import 实现；此处以 trait 抽象，
/// 便于注入不同实现（真实文档引擎 / 测试桩）。
pub trait HostFunctionRegistry: Send + Sync {
    /// 执行宿主函数调用。
    fn call(
        &self,
        function: HostFunction,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, crate::Error>;
}

/// 默认宿主函数实现：内存文档存储 + tracing 日志。
pub struct DefaultHostFunctions {
    docs: Arc<RwLock<HashMap<String, serde_json::Value>>>,
}

impl Default for DefaultHostFunctions {
    fn default() -> Self {
        Self::new()
    }
}

impl DefaultHostFunctions {
    pub fn new() -> Self {
        Self {
            docs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 写入一份测试文档
    pub fn seed(&self, doc_id: impl Into<String>, value: serde_json::Value) {
        self.docs.write().insert(doc_id.into(), value);
    }
}

impl HostFunctionRegistry for DefaultHostFunctions {
    fn call(
        &self,
        function: HostFunction,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, crate::Error> {
        match function {
            HostFunction::ReadDoc => {
                let doc_id = args
                    .get("doc_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| crate::Error::HostFunction("read_doc requires `doc_id`".into()))?;
                let docs = self.docs.read();
                Ok(docs.get(doc_id).cloned().unwrap_or(serde_json::Value::Null))
            }
            HostFunction::WriteDoc => {
                let doc_id = args
                    .get("doc_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| crate::Error::HostFunction("write_doc requires `doc_id`".into()))?;
                let data = args.get("data").cloned().unwrap_or(serde_json::Value::Null);
                self.docs.write().insert(doc_id.to_string(), data);
                Ok(serde_json::json!({"ok": true}))
            }
            HostFunction::Query => {
                let query = args
                    .get("query")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let docs = self.docs.read();
                let matches: Vec<&String> = docs
                    .keys()
                    .filter(|k| query.is_empty() || k.contains(query))
                    .collect();
                Ok(serde_json::json!({ "results": matches }))
            }
            HostFunction::Log => {
                let level = args.get("level").and_then(|v| v.as_str()).unwrap_or("info");
                let message = args.get("message").and_then(|v| v.as_str()).unwrap_or("");
                match level {
                    "warn" => warn!(target: "plugin", "[plugin] {}", message),
                    "error" => tracing::error!(target: "plugin", "[plugin] {}", message),
                    _ => info!(target: "plugin", "[plugin] {}", message),
                }
                Ok(serde_json::json!({"ok": true}))
            }
        }
    }
}

/// WASM 插件运行时：管理插件能力清单并分派宿主函数调用。
///
/// 持有 `wasmtime::Engine` 表示运行时引擎已就绪；宿主函数通过注入的
/// `HostFunctionRegistry` 实现，能力校验在调用前强制执行。
pub struct WasmRuntime {
    engine: wasmtime::Engine,
    registry: Arc<dyn HostFunctionRegistry>,
    manifests: Arc<RwLock<HashMap<PluginId, CapabilityManifest>>>,
}

impl WasmRuntime {
    /// 使用指定宿主函数注册表构造运行时。
    pub fn new(registry: Arc<dyn HostFunctionRegistry>) -> Self {
        Self {
            engine: wasmtime::Engine::default(),
            registry,
            manifests: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 使用默认内存宿主函数构造运行时。
    pub fn with_default_registry() -> Self {
        Self::new(Arc::new(DefaultHostFunctions::new()))
    }

    /// 返回底层 Wasmtime 引擎句柄（生产环境用于模块实例化）。
    pub fn engine(&self) -> &wasmtime::Engine {
        &self.engine
    }

    /// 注册插件及其能力清单。
    pub fn register(&self, plugin_id: impl Into<PluginId>, manifest: CapabilityManifest) {
        let id = plugin_id.into();
        debug!("wasm runtime: register plugin {} with {} capabilities", id, manifest.len());
        self.manifests.write().insert(id, manifest);
    }

    /// 注销插件。
    pub fn unregister(&self, plugin_id: &str) {
        self.manifests.write().remove(plugin_id);
    }

    /// 返回插件能力清单快照。
    pub fn manifest(&self, plugin_id: &str) -> Option<CapabilityManifest> {
        self.manifests.read().get(plugin_id).cloned()
    }

    /// 校验插件是否被授予指定能力。
    pub fn validate_capability(
        &self,
        plugin_id: &str,
        required: Capability,
    ) -> Result<(), crate::Error> {
        let manifest = self
            .manifest(plugin_id)
            .ok_or_else(|| crate::Error::NotFound(format!("plugin not registered: {}", plugin_id)))?;
        manifest.require(required)
    }

    /// 调用宿主函数：先校验能力，再分派到注册表。
    pub fn call_host_function(
        &self,
        plugin_id: &str,
        function: HostFunction,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, crate::Error> {
        if let Some(cap) = function.required_capability() {
            self.validate_capability(plugin_id, cap)?;
        }
        debug!(
            "wasm runtime: plugin {} calls host function {}",
            plugin_id,
            function.name()
        );
        self.registry.call(function, args)
    }

    /// 调用插件导出方法（host → plugin）。
    ///
    /// 若方法名匹配某宿主函数，则按宿主函数分派（模拟插件回调宿主）；
    /// 否则模拟执行插件导出函数并返回确认结果。
    pub fn invoke(
        &self,
        plugin_id: &str,
        method: &str,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, crate::Error> {
        if self.manifest(plugin_id).is_none() {
            return Err(crate::Error::NotFound(format!(
                "plugin not registered: {}",
                plugin_id
            )));
        }
        if let Some(f) = HostFunction::parse(method) {
            return self.call_host_function(plugin_id, f, args);
        }
        debug!(
            "wasm runtime: invoke plugin {} export `{}`",
            plugin_id, method
        );
        Ok(serde_json::json!({
            "ok": true,
            "plugin": plugin_id,
            "method": method,
            "args": args,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capability_parse_and_as_str() {
        assert_eq!(Capability::parse("fs"), Some(Capability::Fs));
        assert_eq!(Capability::parse("network"), Some(Capability::Network));
        assert_eq!(Capability::parse("native"), Some(Capability::Native));
        assert_eq!(Capability::parse("storage"), Some(Capability::Storage));
        assert_eq!(Capability::parse("unknown"), None);

        assert_eq!(Capability::Fs.as_str(), "fs");
        assert_eq!(Capability::Storage.as_str(), "storage");
    }

    #[test]
    fn test_capability_manifest_from_permissions() {
        let m = CapabilityManifest::from_permissions(&[
            "fs".to_string(),
            "network".to_string(),
            "storage".to_string(),
            "ignored".to_string(),
        ]);
        assert!(m.grants(Capability::Fs));
        assert!(m.grants(Capability::Network));
        assert!(m.grants(Capability::Storage));
        assert!(!m.grants(Capability::Native));
        assert_eq!(m.len(), 3);
    }

    #[test]
    fn test_capability_manifest_grant_require() {
        let mut m = CapabilityManifest::new();
        assert!(m.is_empty());
        m.grant(Capability::Native);
        assert!(m.grants(Capability::Native));
        assert!(m.require(Capability::Native).is_ok());
        let err = m.require(Capability::Fs).unwrap_err();
        assert!(matches!(err, crate::Error::CapabilityDenied(Capability::Fs)));
    }

    #[test]
    fn test_host_function_parse_name_capability() {
        assert_eq!(HostFunction::parse("read_doc"), Some(HostFunction::ReadDoc));
        assert_eq!(HostFunction::parse("log"), Some(HostFunction::Log));
        assert_eq!(HostFunction::parse("nope"), None);

        assert_eq!(HostFunction::ReadDoc.name(), "read_doc");
        assert_eq!(
            HostFunction::ReadDoc.required_capability(),
            Some(Capability::Storage)
        );
        assert_eq!(HostFunction::Log.required_capability(), None);
    }

    #[test]
    fn test_default_host_functions_read_write_doc() {
        let rt = WasmRuntime::with_default_registry();
        rt.register("p1", {
            let mut m = CapabilityManifest::new();
            m.grant(Capability::Storage);
            m
        });

        let write = rt
            .call_host_function(
                "p1",
                HostFunction::WriteDoc,
                &serde_json::json!({"doc_id": "d1", "data": {"title": "Hello"}}),
            )
            .unwrap();
        assert_eq!(write["ok"], serde_json::json!(true));

        let read = rt
            .call_host_function("p1", HostFunction::ReadDoc, &serde_json::json!({"doc_id": "d1"}))
            .unwrap();
        assert_eq!(read["title"], serde_json::json!("Hello"));
    }

    #[test]
    fn test_default_host_functions_query_and_log() {
        let rt = WasmRuntime::with_default_registry();
        rt.register("p1", {
            let mut m = CapabilityManifest::new();
            m.grant(Capability::Storage);
            m
        });
        rt.call_host_function(
            "p1",
            HostFunction::WriteDoc,
            &serde_json::json!({"doc_id": "alpha-1", "data": 1}),
        )
        .unwrap();
        rt.call_host_function(
            "p1",
            HostFunction::WriteDoc,
            &serde_json::json!({"doc_id": "beta-2", "data": 2}),
        )
        .unwrap();

        let q = rt
            .call_host_function("p1", HostFunction::Query, &serde_json::json!({"query": "alpha"}))
            .unwrap();
        let results = q["results"].as_array().unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], serde_json::json!("alpha-1"));

        // Log 不需要能力，即便无 Storage 也应成功
        rt.register("p2", CapabilityManifest::new());
        let log = rt
            .call_host_function("p2", HostFunction::Log, &serde_json::json!({"level": "info", "message": "hi"}))
            .unwrap();
        assert_eq!(log["ok"], serde_json::json!(true));
    }

    #[test]
    fn test_wasm_runtime_capability_enforcement() {
        let rt = WasmRuntime::with_default_registry();
        // p1 未授予 Storage
        rt.register("p1", CapabilityManifest::new());

        let err = rt
            .call_host_function(
                "p1",
                HostFunction::ReadDoc,
                &serde_json::json!({"doc_id": "x"}),
            )
            .unwrap_err();
        assert!(matches!(
            err,
            crate::Error::CapabilityDenied(Capability::Storage)
        ));

        // 授予 Storage 后调用成功
        rt.manifests.write().get_mut("p1").unwrap().grant(Capability::Storage);
        let res = rt.call_host_function(
            "p1",
            HostFunction::ReadDoc,
            &serde_json::json!({"doc_id": "x"}),
        );
        assert!(res.is_ok());
    }

    #[test]
    fn test_wasm_runtime_invoke_host_function_dispatch() {
        let rt = WasmRuntime::with_default_registry();
        rt.register("p1", {
            let mut m = CapabilityManifest::new();
            m.grant(Capability::Storage);
            m
        });
        // invoke "write_doc" 应分派到宿主函数
        let out = rt
            .invoke("p1", "write_doc", &serde_json::json!({"doc_id": "z", "data": 42}))
            .unwrap();
        assert_eq!(out["ok"], serde_json::json!(true));

        let read = rt.invoke("p1", "read_doc", &serde_json::json!({"doc_id": "z"})).unwrap();
        assert_eq!(read, serde_json::json!(42));
    }

    #[test]
    fn test_wasm_runtime_invoke_unknown_method() {
        let rt = WasmRuntime::with_default_registry();
        rt.register("p1", CapabilityManifest::new());
        let out = rt.invoke("p1", "render", &serde_json::json!({"x": 1})).unwrap();
        assert_eq!(out["ok"], serde_json::json!(true));
        assert_eq!(out["method"], serde_json::json!("render"));
    }

    #[test]
    fn test_wasm_runtime_unregistered_plugin_errors() {
        let rt = WasmRuntime::with_default_registry();
        let err = rt.validate_capability("ghost", Capability::Fs).unwrap_err();
        assert!(matches!(err, crate::Error::NotFound(_)));
        let err = rt.invoke("ghost", "log", &serde_json::json!({})).unwrap_err();
        assert!(matches!(err, crate::Error::NotFound(_)));
    }

    #[test]
    fn test_wasm_runtime_engine_ready() {
        let rt = WasmRuntime::with_default_registry();
        // 引擎句柄可访问，表明 Wasmtime 运行时已就绪
        let _engine = rt.engine();
    }
}
