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
use std::time::Duration;

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
                let doc_id = args.get("doc_id").and_then(|v| v.as_str()).ok_or_else(|| {
                    crate::Error::HostFunction("read_doc requires `doc_id`".into())
                })?;
                let docs = self.docs.read();
                Ok(docs.get(doc_id).cloned().unwrap_or(serde_json::Value::Null))
            }
            HostFunction::WriteDoc => {
                let doc_id = args.get("doc_id").and_then(|v| v.as_str()).ok_or_else(|| {
                    crate::Error::HostFunction("write_doc requires `doc_id`".into())
                })?;
                let data = args.get("data").cloned().unwrap_or(serde_json::Value::Null);
                self.docs.write().insert(doc_id.to_string(), data);
                Ok(serde_json::json!({"ok": true}))
            }
            HostFunction::Query => {
                let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
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

/// DK-14d.d1：epoch ticker Drop 关停——置停机位后 join，不留孤儿线程
/// （shutdown 路径：进程退出/runtime drop 均触发，见 struct 文档）。
impl Drop for WasmRuntime {
    fn drop(&mut self) {
        use std::sync::atomic::Ordering;
        self.ticker_stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.ticker_handle.take() {
            let _ = handle.join();
        }
    }
}

// DK-14d.d1：最小 WIT 合约绑定（host 侧生成）——宏展开产物在本模块
// 作用域（PluginWorld 等 pub 类型直接可见）。
// 合约文件 `wit/aurora-plugin.wit`；文件头含 capability 参数位与
// DK-14c 的映射说明。world `plugin` 仅 export host 接口（无 import
// → bindgen 不要求宿主侧 trait 实现，编译零负担）。d2 起以此合约
// 类型校验真实 guest component。
wasmtime::component::bindgen!({
    path: "wit",
    world: "plugin",
});

/// WASM 插件运行时：管理插件能力清单并分派宿主函数调用。
///
/// 持有 `wasmtime::Engine`（DK-14d.d1：component model + epoch 抢占配置）
/// 表示运行时引擎已就绪；宿主函数通过注入的 `HostFunctionRegistry`
/// 实现，能力校验在调用前强制执行。
///
/// # epoch ticker 生命周期（工单执行规约）
/// 后台线程每 [`EPOCH_TICK_MS`] ms 调 `Engine::increment_epoch`——
/// shutdown 路径：[`Drop`] 置停机位后 join（进程退出 / runtime drop
/// 均会关停，不留孤儿线程）。
pub struct WasmRuntime {
    engine: wasmtime::Engine,
    registry: Arc<dyn HostFunctionRegistry>,
    manifests: Arc<RwLock<HashMap<PluginId, CapabilityManifest>>>,
    /// epoch ticker 停机位（Drop 关停路径）
    ticker_stop: Arc<std::sync::atomic::AtomicBool>,
    /// ticker 线程句柄（Drop 时 join）
    ticker_handle: Option<std::thread::JoinHandle<()>>,
    /// DK-14d.2：已登记 component 字节（懒实例化源）
    components: Arc<RwLock<HashMap<PluginId, Vec<u8>>>>,
    /// DK-14d.2：实例缓存（plugin → Mutex<PluginInstance>；per-plugin
    /// 锁粒度——同插件 invoke 串行，Store &mut 不可重入）
    instances: Arc<RwLock<HashMap<PluginId, std::sync::Arc<parking_lot::Mutex<PluginInstance>>>>>,
    /// DK-14d.2：epoch deadline ticks（运行时可调，默认 200=10s）
    epoch_deadline_ticks: std::sync::atomic::AtomicU64,
    /// DK-14d.2：实例化计数（缓存命中观测——测试用，生产可作指标）
    instantiations: std::sync::atomic::AtomicU64,
}

/// DK-14d.2：Store 上下文——宿主函数实现（d3 宿主回调接线）侧读取。
/// 字段登记即意图（d3 消费；当前仅挂载未读——挂接清单列明）。
#[allow(dead_code)]
struct WasmStoreCtx {
    plugin_id: String,
    manifest: CapabilityManifest,
}

/// DK-14d.2：已实例化插件（Store+实例句柄；per-plugin Mutex 包裹）。
struct PluginInstance {
    store: wasmtime::Store<WasmStoreCtx>,
    instance: wasmtime::component::Instance,
}

/// DK-14d.2：epoch deadline 默认值（ticks；200×50ms=10s，工单建议）。
const DEFAULT_EPOCH_DEADLINE_TICKS: u64 = 200;

/// epoch 心跳间隔（ms）。deadline 单位=tick 数，50ms 粒度即可支撑
/// 毫秒级死循环打断（DK-14b 单测实证）。
const EPOCH_TICK_MS: u64 = 50;

impl WasmRuntime {
    /// 使用指定宿主函数注册表构造运行时。
    ///
    /// DK-14d.d1 Config 重构（原 `Engine::default()`）：
    /// - `wasm_component_model(true)`——WIT 合约 Component 加载前置；
    /// - `epoch_interruption(true)` + 后台 ticker——死循环 wasm 可被打断
    ///   （DK-14b 并入本项一次重构；Store 级 `set_deadline` 在实例化层接线）；
    /// - ResourceLimiter 错误类型经 workspace 传递依赖显式声明（DK-39
    ///   惯例，workspace Cargo.toml L41 注释），limiter 挂载点在 Store
    ///   创建层（d2 生产接线）。
    pub fn new(registry: Arc<dyn HostFunctionRegistry>) -> Self {
        use std::sync::atomic::AtomicBool;

        let mut config = wasmtime::Config::new();
        config.wasm_component_model(true);
        config.epoch_interruption(true);
        let engine = wasmtime::Engine::new(&config).expect("engine config valid");

        // epoch ticker 后台线程（Drop 关停 + join，见 struct 文档）
        let ticker_stop = Arc::new(AtomicBool::new(false));
        let stop_flag = ticker_stop.clone();
        let ticker_engine = engine.clone();
        let ticker_handle = std::thread::Builder::new()
            .name("wasm-epoch-ticker".into())
            .spawn(move || {
                use std::sync::atomic::Ordering;
                while !stop_flag.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(EPOCH_TICK_MS));
                    ticker_engine.increment_epoch();
                }
            })
            .expect("spawn wasm-epoch-ticker thread");

        Self {
            engine,
            registry,
            manifests: Arc::new(RwLock::new(HashMap::new())),
            ticker_stop,
            ticker_handle: Some(ticker_handle),
            components: Arc::new(RwLock::new(HashMap::new())),
            instances: Arc::new(RwLock::new(HashMap::new())),
            epoch_deadline_ticks: std::sync::atomic::AtomicU64::new(DEFAULT_EPOCH_DEADLINE_TICKS),
            instantiations: std::sync::atomic::AtomicU64::new(0),
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

    /// DK-14d.d1：真实 WASM Component 加载（component model API）。
    ///
    /// 验证 wasm 字节是否为合法 component——d2 真执行链的加载前置
    /// （wasm_runtime L331 区，本卡零触碰）。加载失败返回明确错误
    /// （无效字节/类型节损坏均走 [`crate::Error::WasmRuntime`]）。
    pub fn load_component(
        &self,
        wasm_bytes: &[u8],
    ) -> Result<wasmtime::component::Component, crate::Error> {
        wasmtime::component::Component::new(&self.engine, wasm_bytes)
            .map_err(|e| crate::Error::WasmRuntime(format!("component load failed: {e}")))
    }

    /// 注册插件及其能力清单。
    pub fn register(&self, plugin_id: impl Into<PluginId>, manifest: CapabilityManifest) {
        let id = plugin_id.into();
        debug!(
            "wasm runtime: register plugin {} with {} capabilities",
            id,
            manifest.len()
        );
        self.manifests.write().insert(id, manifest);
    }

    /// DK-14d.2：登记插件 wasm 字节（component）——首次 invoke 时懒实例化。
    ///
    /// 生产接线：真实插件包分发后调用本方法登记；测试用 wat 编译产物。
    /// 重复登记以最新字节为准并清除既有实例（下次 invoke 重新实例化）。
    pub fn install_component(&self, plugin_id: &str, wasm_bytes: Vec<u8>) {
        self.components
            .write()
            .insert(plugin_id.to_string(), wasm_bytes);
        self.instances.write().remove(plugin_id);
    }

    /// 注销插件。
    pub fn unregister(&self, plugin_id: &str) {
        self.manifests.write().remove(plugin_id);
        // DK-14d.2：实例层同步 Drop（Store+instance 释放；再注册即重新实例化）
        self.instances.write().remove(plugin_id);
        self.components.write().remove(plugin_id);
    }

    /// DK-14d.2：epoch deadline 运行时可调（ticks；1 tick=50ms）。
    ///
    /// 默认 200 ticks=10s（工单建议值）；测试用小值避免挂死。
    pub fn set_epoch_deadline_ticks(&self, ticks: u64) {
        self.epoch_deadline_ticks
            .store(ticks, std::sync::atomic::Ordering::Relaxed);
    }

    /// DK-14d.2：capability 串派生——注册 Manifest 的 granted 集合逗号连接。
    ///
    /// 14c 资源化时替换为真 capability token（合约文件头有映射说明）。
    fn derive_capability_string(manifest: &CapabilityManifest) -> String {
        const ALL: [Capability; 4] = [
            Capability::Fs,
            Capability::Network,
            Capability::Native,
            Capability::Storage,
        ];
        ALL.iter()
            .filter(|c| manifest.grants(**c))
            .map(|c| c.as_str())
            .collect::<Vec<_>>()
            .join(",")
    }

    /// DK-14d.2：懒实例化（缓存命中直接返回）。
    ///
    /// 全链：load_component → Store::new(WasmStoreCtx) → set_epoch_deadline →
    /// component Linker → instantiate。per-plugin Mutex 粒度：同一插件
    /// invoke 串行（Store &mut 借用不可重入），不同插件互不阻塞。
    fn get_or_instantiate(
        &self,
        plugin_id: &str,
    ) -> Result<std::sync::Arc<parking_lot::Mutex<PluginInstance>>, crate::Error> {
        use std::sync::atomic::Ordering;
        // 快路径：已实例化
        if let Some(hit) = self.instances.read().get(plugin_id) {
            return Ok(hit.clone());
        }
        // 慢路径：登记字节 → 加载 → 实例化
        let bytes = self
            .components
            .read()
            .get(plugin_id)
            .cloned()
            .ok_or_else(|| {
                crate::Error::NotFound(format!("plugin component not installed: {}", plugin_id))
            })?;
        let manifest = self.manifest(plugin_id).ok_or_else(|| {
            crate::Error::NotFound(format!("plugin not registered: {}", plugin_id))
        })?;
        let component = self.load_component(&bytes)?;
        let mut store = wasmtime::Store::new(
            &self.engine,
            WasmStoreCtx {
                plugin_id: plugin_id.to_string(),
                manifest,
            },
        );
        store.set_epoch_deadline(self.epoch_deadline_ticks.load(Ordering::Relaxed));
        let linker = wasmtime::component::Linker::<WasmStoreCtx>::new(&self.engine);
        let instance = linker
            .instantiate(&mut store, &component)
            .map_err(|e| crate::Error::WasmRuntime(format!("component instantiate failed: {e}")))?;
        let inst = std::sync::Arc::new(parking_lot::Mutex::new(PluginInstance { store, instance }));
        self.instantiations.fetch_add(1, Ordering::Relaxed);
        self.instances
            .write()
            .insert(plugin_id.to_string(), inst.clone());
        Ok(inst)
    }

    /// DK-14d.2：真执行链——调用插件导出函数（host → plugin 导出）。
    ///
    /// result 检查：guest 返回 err → `Error::WasmRuntime`；成功返回确认
    /// JSON（真调用后，非模拟确认）。typed 调用（强类型参数映射）；
    /// Store 级 epoch deadline 在每次 wasm 调用进入时以当前 epoch 为
    /// 基线生效（懒计算）。
    fn call_export_init(
        &self,
        plugin_id: &str,
        capability: String,
    ) -> Result<serde_json::Value, crate::Error> {
        let inst = self.get_or_instantiate(plugin_id)?;
        let mut guard = inst.lock();
        let PluginInstance { store, instance } = &mut *guard;
        let func = instance
            .get_typed_func::<(&str,), (Result<(), ()>,)>(&mut *store, "init")
            .map_err(|e| {
                crate::Error::WasmRuntime(format!("component lacks export `init`: {e}"))
            })?;
        let (r,) = func
            .call(&mut *store, (capability.as_str(),))
            .map_err(|e| crate::Error::WasmRuntime(format!("export `init` call failed: {e}")))?;
        r.map_err(|()| crate::Error::WasmRuntime("export `init` returned error".into()))?;
        // post-return：解锁实例（guest 声明 post-return 时必调，否则
        // 二次 invoke 报 "cannot enter component instance"——DK-14d.2 实证）
        func.post_return(&mut *store).map_err(|e| {
            crate::Error::WasmRuntime(format!("export `init` post-return failed: {e}"))
        })?;
        Ok(serde_json::json!({
            "ok": true,
            "plugin": plugin_id,
            "method": "init",
        }))
    }

    /// DK-14d.2：handle-event 导出调用（三 string 参数）。
    fn call_export_handle_event(
        &self,
        plugin_id: &str,
        capability: String,
        event: String,
        payload: String,
    ) -> Result<serde_json::Value, crate::Error> {
        let inst = self.get_or_instantiate(plugin_id)?;
        let mut guard = inst.lock();
        let PluginInstance { store, instance } = &mut *guard;
        let func = instance
            .get_typed_func::<(&str, &str, &str), (Result<(), ()>,)>(&mut *store, "handle-event")
            .map_err(|e| {
                crate::Error::WasmRuntime(format!("component lacks export `handle-event`: {e}"))
            })?;
        let (r,) = func
            .call(
                &mut *store,
                (capability.as_str(), event.as_str(), payload.as_str()),
            )
            .map_err(|e| {
                crate::Error::WasmRuntime(format!("export `handle-event` call failed: {e}"))
            })?;
        r.map_err(|()| crate::Error::WasmRuntime("export `handle-event` returned error".into()))?;
        // post-return：解锁实例（同 init 注释）
        func.post_return(&mut *store).map_err(|e| {
            crate::Error::WasmRuntime(format!("export `handle-event` post-return failed: {e}"))
        })?;
        Ok(serde_json::json!({
            "ok": true,
            "plugin": plugin_id,
            "method": "handle-event",
        }))
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
        let manifest = self.manifest(plugin_id).ok_or_else(|| {
            crate::Error::NotFound(format!("plugin not registered: {}", plugin_id))
        })?;
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
    /// 双路语义（DK-14d.2 模拟段划界后的最终形态）：
    /// - `HostFunction::parse` 命中 → 宿主函数分派（真实：registry.call
    ///   + capability 校验，插件回调宿主通道）；
    /// - `init` / `handle-event` → 真执行链（懒实例化 + component 导出
    ///   调用；capability 串由注册 Manifest 派生，14c 换真 token）；
    /// - 其余 → `InvalidInput`（NotSupported——不再假 ok）。
    pub fn invoke(
        &self,
        plugin_id: &str,
        method: &str,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, crate::Error> {
        let manifest = self.manifest(plugin_id).ok_or_else(|| {
            crate::Error::NotFound(format!("plugin not registered: {}", plugin_id))
        })?;
        if let Some(f) = HostFunction::parse(method) {
            return self.call_host_function(plugin_id, f, args);
        }
        match method {
            "init" => {
                let capability = Self::derive_capability_string(&manifest);
                self.call_export_init(plugin_id, capability)
            }
            "handle-event" => {
                let capability = Self::derive_capability_string(&manifest);
                let event = args
                    .get("event")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                let payload = match args.get("payload") {
                    Some(serde_json::Value::String(s)) => s.clone(),
                    Some(other) => other.to_string(),
                    None => String::new(),
                };
                self.call_export_handle_event(plugin_id, capability, event, payload)
            }
            other => Err(crate::Error::InvalidInput(format!(
                "method not supported: {}",
                other
            ))),
        }
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
        assert!(matches!(
            err,
            crate::Error::CapabilityDenied(Capability::Fs)
        ));
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
            .call_host_function(
                "p1",
                HostFunction::ReadDoc,
                &serde_json::json!({"doc_id": "d1"}),
            )
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
            .call_host_function(
                "p1",
                HostFunction::Query,
                &serde_json::json!({"query": "alpha"}),
            )
            .unwrap();
        let results = q["results"].as_array().unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], serde_json::json!("alpha-1"));

        // Log 不需要能力，即便无 Storage 也应成功
        rt.register("p2", CapabilityManifest::new());
        let log = rt
            .call_host_function(
                "p2",
                HostFunction::Log,
                &serde_json::json!({"level": "info", "message": "hi"}),
            )
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
        rt.manifests
            .write()
            .get_mut("p1")
            .unwrap()
            .grant(Capability::Storage);
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
            .invoke(
                "p1",
                "write_doc",
                &serde_json::json!({"doc_id": "z", "data": 42}),
            )
            .unwrap();
        assert_eq!(out["ok"], serde_json::json!(true));

        let read = rt
            .invoke("p1", "read_doc", &serde_json::json!({"doc_id": "z"}))
            .unwrap();
        assert_eq!(read, serde_json::json!(42));
    }

    #[test]
    fn test_wasm_runtime_invoke_unknown_method() {
        let rt = WasmRuntime::with_default_registry();
        rt.register("p1", CapabilityManifest::new());
        // DK-14d.2 裁决：非 init/handle-event 且非宿主函数 → NotSupported
        // （原假确认 JSON 段已删除）
        let err = rt
            .invoke("p1", "render", &serde_json::json!({"x": 1}))
            .unwrap_err();
        match err {
            crate::Error::InvalidInput(m) => {
                assert!(m.contains("not supported"), "错误需可定位: {m}");
            }
            other => panic!("应为 InvalidInput，实际: {other:?}"),
        }
    }

    #[test]
    fn test_wasm_runtime_unregistered_plugin_errors() {
        let rt = WasmRuntime::with_default_registry();
        let err = rt.validate_capability("ghost", Capability::Fs).unwrap_err();
        assert!(matches!(err, crate::Error::NotFound(_)));
        let err = rt
            .invoke("ghost", "log", &serde_json::json!({}))
            .unwrap_err();
        assert!(matches!(err, crate::Error::NotFound(_)));
    }

    // ===== DK-14d.2：真执行链（模拟段替换）=====

    /// 最小 guest component 字节（wat→binary 贴近生产路径）。
    fn minimal_guest() -> Vec<u8> {
        wat::parse_str(include_str!("../tests/fixtures/guest_minimal.wat"))
            .expect("guest_minimal.wat 语法合法")
    }

    fn registered_runtime_with_guest() -> (WasmRuntime, &'static str) {
        let rt = WasmRuntime::with_default_registry();
        rt.register("p1", {
            let mut m = CapabilityManifest::new();
            m.grant(Capability::Storage);
            m.grant(Capability::Network);
            m
        });
        rt.install_component("p1", minimal_guest());
        (rt, "p1")
    }

    /// DoD：invoke("init") 全链通过——load→Store→Linker→instantiate→call
    /// （capability 串由 Manifest 派生，真调用成功返回确认 JSON）。
    #[test]
    fn test_invoke_init_full_chain() {
        let (rt, id) = registered_runtime_with_guest();
        let out = rt.invoke(id, "init", &serde_json::json!({})).unwrap();
        assert_eq!(out["ok"], serde_json::json!(true));
        assert_eq!(out["plugin"], serde_json::json!(id));
        assert_eq!(out["method"], serde_json::json!("init"));
    }

    /// DoD：invoke("handle-event") 全链通过（event/payload 参数提取）。
    #[test]
    fn test_invoke_handle_event_full_chain() {
        let (rt, id) = registered_runtime_with_guest();
        let out = rt
            .invoke(
                id,
                "handle-event",
                &serde_json::json!({"event": "doc_saved", "payload": {"id": 1}}),
            )
            .unwrap();
        assert_eq!(out["ok"], serde_json::json!(true));
        assert_eq!(out["method"], serde_json::json!("handle-event"));
    }

    /// DoD：实例缓存命中——二次 invoke 不重复实例化（instantiations 计数=1）。
    #[test]
    fn test_instance_cache_hit() {
        let (rt, id) = registered_runtime_with_guest();
        rt.invoke(id, "init", &serde_json::json!({})).unwrap();
        rt.invoke(id, "handle-event", &serde_json::json!({}))
            .unwrap();
        rt.invoke(id, "init", &serde_json::json!({})).unwrap();
        assert_eq!(
            rt.instantiations.load(std::sync::atomic::Ordering::Relaxed),
            1,
            "三次 invoke 共享一次实例化"
        );
    }

    /// 挂接自查：unregister 清实例缓存——再注册+invoke 重新实例化（计数=2）。
    #[test]
    fn test_unregister_drops_instance_cache() {
        let (rt, id) = registered_runtime_with_guest();
        rt.invoke(id, "init", &serde_json::json!({})).unwrap();
        assert_eq!(
            rt.instantiations.load(std::sync::atomic::Ordering::Relaxed),
            1
        );
        rt.unregister(id);
        // 未注册 → NotFound（行为保持）
        let err = rt.invoke(id, "init", &serde_json::json!({})).unwrap_err();
        assert!(matches!(err, crate::Error::NotFound(_)));
        // 再注册+登记 → 重新实例化
        rt.register(id, CapabilityManifest::new());
        rt.install_component(id, minimal_guest());
        rt.invoke(id, "init", &serde_json::json!({})).unwrap();
        assert_eq!(
            rt.instantiations.load(std::sync::atomic::Ordering::Relaxed),
            2,
            "unregister 后缓存已清，重新实例化"
        );
    }

    /// DoD：epoch deadline 对 guest 生效——死循环 component 被 epoch
    /// 打断 → 明确错误返回（Store 级接线，ticks 运行时可调）。
    #[test]
    fn test_epoch_deadline_kills_spin_guest() {
        let rt = WasmRuntime::with_default_registry();
        rt.set_epoch_deadline_ticks(2); // 2 tick（~100ms）后打断
        rt.register("spin", CapabilityManifest::new());
        rt.install_component(
            "spin",
            wat::parse_str(include_str!("../tests/fixtures/guest_spin.wat"))
                .expect("guest_spin.wat 语法合法"),
        );
        let err = rt
            .invoke(
                "spin",
                "handle-event",
                &serde_json::json!({"event": "x", "payload": ""}),
            )
            .unwrap_err();
        match err {
            crate::Error::WasmRuntime(m) => {
                assert!(m.contains("call failed"), "错误需可定位: {m}");
            }
            other => panic!("应为 WasmRuntime 错误，实际: {other:?}"),
        }
    }

    /// 未登记 component 的已注册插件 → NotFound（懒实例化前置缺失）。
    #[test]
    fn test_invoke_without_installed_component() {
        let rt = WasmRuntime::with_default_registry();
        rt.register("p2", CapabilityManifest::new());
        let err = rt.invoke("p2", "init", &serde_json::json!({})).unwrap_err();
        assert!(matches!(err, crate::Error::NotFound(_)));
    }

    #[test]
    fn test_wasm_runtime_engine_ready() {
        let rt = WasmRuntime::with_default_registry();
        // 引擎句柄可访问，表明 Wasmtime 运行时已就绪
        let _engine = rt.engine();
    }

    // ===== DK-14d.d1：Config 重构 + Component 加载 + WIT 合约 =====

    /// DoD：epoch deadline 打断死循环 wasm（DK-14b 并入验证——
    /// fixture = wat 文本编译的 core module 死循环，wat 纯库非工具链）。
    #[test]
    fn test_epoch_deadline_interrupts_infinite_loop() {
        let rt = WasmRuntime::with_default_registry();
        let mut store = wasmtime::Store::new(&rt.engine, ());
        let wat = r#"(module (func (export "spin") (loop br 0)))"#;
        let module = wasmtime::Module::new(&rt.engine, wat).expect("wat module valid");
        let instance = wasmtime::Linker::<()>::new(&rt.engine)
            .instantiate(&mut store, &module)
            .expect("instantiate");
        let spin = instance
            .get_typed_func::<(), ()>(&mut store, "spin")
            .expect("spin export");

        // 当前 epoch + 2 tick（~100ms）后强制打断——ticker 线程在推进 epoch
        store.set_epoch_deadline(2);

        let err = spin.call(&mut store, ()).expect_err("死循环必须被打断");
        // epoch 打断根因在错误因果链深处（wasmtime 36 无 Trap::Interrupted
        // 变体——Debug 格式展开完整链后按 interrupt/epoch 关键词断言）
        let msg = format!("{err:?}").to_lowercase();
        assert!(
            msg.contains("interrupt") || msg.contains("epoch"),
            "应为 epoch 打断 trap，实际: {err:?}"
        );
    }

    /// DoD：Component::new 加载真实 wasm 成功（component model 配置生效——
    /// 极简合法 component fixture，wat→binary 贴近生产二进制路径）。
    #[test]
    fn test_component_new_success() {
        let rt = WasmRuntime::with_default_registry();
        let wat = r#"(component (core module $m (memory 1)) (core instance $i (instantiate $m)))"#;
        let bytes = wat::parse_str(wat).expect("wat parses");
        let comp = match rt.load_component(&bytes) {
            Ok(c) => c,
            Err(e) => panic!("合法 component 必须加载成功: {e}"),
        };
        let _ = comp; // 类型即证明：wasmtime::component::Component
    }

    /// DoD：失败路径返回明确错误（垃圾字节 → Error::WasmRuntime）。
    #[test]
    fn test_component_new_invalid_bytes_error() {
        let rt = WasmRuntime::with_default_registry();
        let err = match rt.load_component(&[0xDE, 0xAD, 0xBE, 0xEF]) {
            Err(e) => e,
            Ok(_) => panic!("垃圾字节不应加载成功"),
        };
        match err {
            crate::Error::WasmRuntime(m) => {
                assert!(m.contains("component load failed"), "错误需可定位: {m}");
            }
            other => panic!("应为 WasmRuntime 错误，实际: {other:?}"),
        }
    }

    /// DoD：WIT 合约文件在库 + bindgen 编译过 + capability 参数位在签名
    /// （签名断言：合约文件 init/handle-event 均含 capability 参数——
    /// 14c 前置契约；bindgen 宏编译通过即产物类型面生效）。
    #[test]
    fn test_wit_contract_capability_signature() {
        // 合约签名面：capability 参数位在两个函数签名中（前缀断言——
        // init 单参带右括号；handle-event 后随更多参数）
        let wit = include_str!("../wit/aurora-plugin.wit");
        for sig in [
            "init: func(capability: string)",
            "handle-event: func(capability: string",
        ] {
            assert!(wit.contains(sig), "WIT 缺 capability 签名: {sig}");
        }
        assert!(wit.contains("world plugin"), "world 定义在库");
    }

    /// DoD：epoch ticker 关停路径——Drop 置停机位+join（结构保证：
    /// drop 返回即线程已停，无孤儿线程泄漏）。
    #[test]
    fn test_epoch_ticker_stops_on_drop() {
        let rt = WasmRuntime::with_default_registry();
        drop(rt); // Drop 内部 join——若死锁/泄漏此测试挂起
                  // 二次 drop 语义：Option<JoinHandle> take 后不重复 join ✓
    }

    /// ResourceLimiter 接线验证（DK-39 经验：错误类型走 workspace 传递
    /// 依赖）——Store 级 limiter 通路可用，内存超限返回明确错误。
    #[test]
    fn test_resource_limiter_store_wiring() {
        struct Cap1Mib;
        impl wasmtime::ResourceLimiter for Cap1Mib {
            fn memory_growing(
                &mut self,
                _current: usize,
                desired: usize,
                _maximum: Option<usize>,
            ) -> Result<bool, wasmtime::Error> {
                Ok(desired <= 1024 * 1024)
            }
            fn table_growing(
                &mut self,
                _current: usize,
                _desired: usize,
                _maximum: Option<usize>,
            ) -> Result<bool, wasmtime::Error> {
                Ok(true)
            }
        }
        let rt = WasmRuntime::with_default_registry();
        let mut store = wasmtime::Store::new(&rt.engine, Cap1Mib);
        store.limiter(|state: &mut Cap1Mib| state);
        // 4MiB 巨型 memory 声明被 limiter 拒绝
        let wat = r#"(module (memory (export "m") 64))"#; // 64 页 = 4MiB
        let module = wasmtime::Module::new(&rt.engine, wat).expect("wat valid");
        let err = wasmtime::Linker::<Cap1Mib>::new(&rt.engine)
            .instantiate(&mut store, &module)
            .expect_err("超限 memory 必须被 limiter 拒绝");
        assert!(
            format!("{err}").to_lowercase().contains("memory"),
            "应为 memory 超限错误: {err}"
        );
    }
}
