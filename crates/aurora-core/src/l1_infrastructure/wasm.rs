//! WASM 运行时 (基于 Wasmtime)
//!
//! 提供安全沙箱化的 WebAssembly 运行时能力，用于隔离执行插件代码。
//! 底层使用 [Wasmtime](https://wasmtime.dev/) 实现。

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::traits::plugin_runtime::{PluginHandle, PluginManifest, PluginRuntime, RuntimeType};

/// DK-39: epoch tick 周期（ms）——共享 Engine 全局单调递增。
const EPOCH_TICK_MS: u64 = 10;
/// DK-39: per-call 缺省 deadline（ticks）——200×10ms = 2s。
pub const DEFAULT_DEADLINE_TICKS: u32 = 200;
/// DK-39: 缺省单插件线性内存上限（64 MiB）。
pub const DEFAULT_MAX_MEMORY_BYTES: usize = 64 * 1024 * 1024;

/// DK-39: Store state 携带内存限额——`Store<MemoryLimiter>` + `store.limiter(|s| s)`。
struct MemoryLimiter {
    max_bytes: usize,
    /// DK-42: 本 call 曾触顶旗标（软拒绝下 grow 返 -1 无 trap——宿主经
    /// last_call_hit_limit 程序化查询触顶，可观测性补齐）。
    hit_limit: bool,
}

impl wasmtime::ResourceLimiter for MemoryLimiter {
    fn memory_growing(
        &mut self,
        _current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> Result<bool, anyhow::Error> {
        // Ok(false) = 拒绝增长 → guest 侧 memory.grow 返回 -1（软失败语义）。
        // DK-42: 拒绝同时置触顶旗标（TA 折中裁决：软语义兼容 + 可观测补齐）。
        let allow = desired <= self.max_bytes;
        if !allow {
            self.hit_limit = true;
        }
        Ok(allow)
    }

    fn table_growing(
        &mut self,
        _current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> Result<bool, wasmtime::Error> {
        // 与 wasmtime 缺省表上限（10 000 行）对齐。
        Ok(desired <= 10_000)
    }
}

/// 基于 Wasmtime 的 WASM 运行时实现。
pub struct WasmtimeRuntime {
    // V26 DK-14 插件生态迭代使用（多模块共享同一 Engine）
    engine: wasmtime::Engine,
    modules: Mutex<HashMap<String, wasmtime::Module>>,
    stores: Mutex<HashMap<String, wasmtime::Store<()>>>,
    /// DK-39: per-call epoch deadline（ticks，10ms/tick）。
    deadline_ticks: u32,
    /// DK-39: 单插件线性内存上限（字节）。
    max_memory_bytes: usize,
    /// DK-39: epoch ticker 停机旗标（Drop 置位）。
    ticker_stop: Arc<AtomicBool>,
    /// DK-42: 最近一次 invoke 是否内存触顶（per-plugin 触顶观测）。
    hit_limits: Mutex<HashMap<String, bool>>,
}

impl WasmtimeRuntime {
    /// 创建新的 Wasmtime 运行时实例（DK-39 缺省限额：2s deadline + 64 MiB 内存）。
    pub fn new() -> Result<Self, crate::Error> {
        Self::with_limits(DEFAULT_DEADLINE_TICKS, DEFAULT_MAX_MEMORY_BYTES)
    }

    /// DK-39: 可配限额构造器（deadline_ticks×10ms；max_memory_bytes 单插件内存上限）。
    /// V1 为 runtime 级限额（缺省即安全），manifest 级覆盖为后续卡。
    pub fn with_limits(deadline_ticks: u32, max_memory_bytes: usize) -> Result<Self, crate::Error> {
        let mut config = wasmtime::Config::new();
        // DK-39: epoch interruption——死循环插件在 deadline 内被 trap，
        // 不再永久阻塞调用线程（插件市场硬前置，见 DK-33 验收注记）。
        config.epoch_interruption(true);
        let engine = wasmtime::Engine::new(&config)
            .map_err(|e| crate::Error::Internal(format!("wasmtime engine: {e}")))?;
        // 后台 epoch ticker：共享 Engine 全局单调递增；per-call
        // Store::set_epoch_deadline(ticks) 相对当前 epoch 计算，插件间互不干扰。
        let stop = Arc::new(AtomicBool::new(false));
        let ticker_engine = engine.clone();
        let stop_flag = Arc::clone(&stop);
        std::thread::Builder::new()
            .name("wasmtime-epoch-ticker".into())
            .spawn(move || {
                while !stop_flag.load(Ordering::Relaxed) {
                    std::thread::sleep(std::time::Duration::from_millis(EPOCH_TICK_MS));
                    ticker_engine.increment_epoch();
                }
            })
            .map_err(|e| crate::Error::Internal(format!("epoch ticker spawn: {e}")))?;
        Ok(Self {
            engine,
            modules: Mutex::new(HashMap::new()),
            stores: Mutex::new(HashMap::new()),
            deadline_ticks,
            max_memory_bytes,
            ticker_stop: stop,
            hit_limits: Mutex::new(HashMap::new()),
        })
    }

    /// DK-42: 查询插件最近一次 invoke 是否发生内存触顶（软拒绝下 grow 返 -1，
    /// 宿主据此程序化感知"插件可能静默行为异常"）。
    pub fn last_call_hit_limit(&self, plugin_id: &str) -> bool {
        self.hit_limits
            .lock()
            .ok()
            .and_then(|m| m.get(plugin_id).copied())
            .unwrap_or(false)
    }
}

impl Drop for WasmtimeRuntime {
    fn drop(&mut self) {
        self.ticker_stop.store(true, Ordering::Relaxed);
    }
}

impl Default for WasmtimeRuntime {
    fn default() -> Self {
        Self::new().expect("wasmtime engine creation should not fail")
    }
}

/// DK-42: invoke 错误分类——epoch 超期与通用失败程序化区分（可观测性）。
fn classify_call_err(e: wasmtime::Error) -> crate::Error {
    if e.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::Interrupt) {
        crate::Error::Internal("plugin deadline exceeded".to_string())
    } else {
        crate::Error::Internal(format!("wasm call: {e}"))
    }
}

/// DK-42: 记录触顶观测（软拒绝无错误——宿主经 last_call_hit_limit 查询）。
fn record_hit_limit(
    hit_limits: &Mutex<HashMap<String, bool>>,
    plugin_id: &str,
    store: &wasmtime::Store<MemoryLimiter>,
) {
    let hit = store.data().hit_limit;
    if hit {
        tracing::warn!("wasm plugin hit memory limit: plugin={plugin_id}（软拒绝：grow 返 -1）");
    }
    if let Ok(mut m) = hit_limits.lock() {
        m.insert(plugin_id.to_string(), hit);
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
        // DK-39: Store state 携带内存限额 + epoch deadline（超时 trap，
        // 恶意/缺陷插件不再阻塞调用线程）。
        // DK-42: manifest.sandbox per-plugin 覆盖（0 = 回退 runtime 缺省；
        // serde default 旧清单自动填缺省安全值 2s/64MiB）。
        let limits = handle.manifest.sandbox;
        let deadline = if limits.epoch_deadline_ticks == 0 {
            self.deadline_ticks
        } else {
            limits.epoch_deadline_ticks
        };
        let max_bytes = if limits.max_memory_bytes == 0 {
            self.max_memory_bytes
        } else {
            limits.max_memory_bytes
        };
        let mut store = wasmtime::Store::new(
            &self.engine,
            MemoryLimiter {
                max_bytes,
                hit_limit: false,
            },
        );
        store.limiter(|state: &mut MemoryLimiter| state);
        store.set_epoch_deadline(u64::from(deadline));
        let linker = wasmtime::Linker::<MemoryLimiter>::new(&self.engine);
        let instance = linker
            .instantiate(&mut store, &module)
            .map_err(|e| crate::Error::Internal(format!("wasm instantiate: {e}")))?;
        let value = match args.as_i64() {
            Some(v) => {
                let f = instance
                    .get_typed_func::<i32, i32>(&mut store, method)
                    .map_err(|e| crate::Error::Internal(format!("wasm export (i32)->i32: {e}")))?;
                let r = f.call(&mut store, v.clamp(i32::MIN as i64, i32::MAX as i64) as i32);
                record_hit_limit(&self.hit_limits, &handle.id, &store);
                let r = r.map_err(classify_call_err)?;
                serde_json::Value::from(r)
            }
            None => {
                let f = instance
                    .get_typed_func::<(), i32>(&mut store, method)
                    .map_err(|e| crate::Error::Internal(format!("wasm export ()->i32: {e}")))?;
                let r = f.call(&mut store, ());
                record_hit_limit(&self.hit_limits, &handle.id, &store);
                let r = r.map_err(classify_call_err)?;
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
    use crate::traits::plugin_runtime::{PluginManifest, RuntimeType, SandboxLimits};

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

    /// DK-39 恶意样本（wat 内联编译）：
    /// - `spin`：死循环（epoch deadline 必须 trap，不阻塞调用线程）
    /// - `eat`：线性内存 grow 循环（limiter 拒绝后 guest 检测 -1 → unreachable trap）
    /// - `answer`：正常导出——验证 trap 后 runtime 继续可用
    fn write_hostile_wasm(dir: &std::path::Path, name: &str) -> String {
        let wasm = wat::parse_str(
            r#"
            (module
                (memory 1)
                (func (export "spin") (result i32) (loop $l (br $l)) unreachable)
                (func (export "eat") (result i32)
                    (loop $l
                        (br_if $l (i32.ne (memory.grow (i32.const 200)) (i32.const -1)))
                    )
                    unreachable
                )
                (func (export "answer") (result i32) (i32.const 42))
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
            sandbox: SandboxLimits::default(),
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

    /// DK-39 DoD 1：死循环插件在 deadline 内 trap 报错，runtime 继续服务。
    #[tokio::test]
    async fn dk39_deadline_spin_traps_and_runtime_recovers() {
        let dir = tempfile::tempdir().unwrap();
        let entry = write_hostile_wasm(dir.path(), "hostile.wasm");
        let mut rt = WasmtimeRuntime::new().unwrap(); // 缺省 200 ticks = 2s
        let handle = rt.load(&manifest(entry)).await.unwrap();

        let start = std::time::Instant::now();
        let out = rt.invoke(&handle, "spin", &serde_json::Value::Null).await;
        let elapsed = start.elapsed();
        assert!(
            out.is_err(),
            "死循环必须在 deadline 内 trap，实际正常返回: {out:?}"
        );
        assert!(
            elapsed < std::time::Duration::from_secs(6),
            "deadline ≈2s，实测 {elapsed:?}——调用线程疑似被阻塞"
        );

        // trap 后 runtime 继续服务：同插件正常导出仍可调用。
        let out = rt
            .invoke(&handle, "answer", &serde_json::Value::Null)
            .await
            .expect("trap 后 runtime 必须继续可用");
        assert_eq!(out, serde_json::json!(42));
    }

    /// DK-39 DoD 2：内存超限插件报错返回（limiter 拒绝 → guest trap）。
    #[tokio::test]
    async fn dk39_memory_limiter_traps_on_exceed() {
        let dir = tempfile::tempdir().unwrap();
        let entry = write_hostile_wasm(dir.path(), "hostile.wasm");
        // 1 MiB 限额：初始 1 页（64 KiB）后首次 grow(200 页) 即超限。
        let mut rt = WasmtimeRuntime::with_limits(DEFAULT_DEADLINE_TICKS, 1024 * 1024).unwrap();
        let handle = rt.load(&manifest(entry)).await.unwrap();

        let out = rt.invoke(&handle, "eat", &serde_json::Value::Null).await;
        assert!(out.is_err(), "内存超限必须报错，实际正常返回: {out:?}");
    }

    /// DK-42 DoD1: manifest 级限额覆盖 + 触顶观测（TA 折中裁决：
    /// 软拒绝语义不变 + hit_limit 位程序化可查）。
    #[tokio::test]
    async fn dk42_manifest_hit_limit_observable() {
        let dir = tempfile::tempdir().unwrap();
        let wasm = wat::parse_str(
            r#"
            (module
                (memory 1)
                (func (export "eat") (result i32)
                    (loop $l
                        (br_if $l (i32.ne (memory.grow (i32.const 16)) (i32.const -1)))
                    )
                    unreachable
                )
                (func (export "answer") (result i32) (i32.const 42))
            )
        "#,
        )
        .unwrap();
        let entry = dir.path().join("dk42.wasm");
        std::fs::write(&entry, wasm).unwrap();

        let mut m = manifest(entry.to_string_lossy().to_string());
        // manifest 级覆盖：256 KB（初始 1 页 64 KiB，数轮 grow 即触顶）
        m.sandbox = SandboxLimits {
            epoch_deadline_ticks: DEFAULT_DEADLINE_TICKS,
            max_memory_bytes: 256 * 1024,
        };
        let mut rt = WasmtimeRuntime::new().unwrap();
        let handle = rt.load(&m).await.unwrap();

        // 软拒绝：grow 返 -1 → guest unreachable trap（既有语义不变，报错返回）
        let out = rt.invoke(&handle, "eat", &serde_json::Value::Null).await;
        assert!(out.is_err(), "超限样本须报错: {out:?}");
        // 触顶可观测：宿主程序化查询本 call 曾触顶
        assert!(rt.last_call_hit_limit("test-plugin"), "触顶旗标必须置位");
        // 未触顶查询：正常调用后旗标复位
        let ok = rt
            .invoke(&handle, "answer", &serde_json::Value::Null)
            .await
            .unwrap();
        assert_eq!(ok, serde_json::json!(42));
        assert!(
            !rt.last_call_hit_limit("test-plugin"),
            "正常调用后触顶旗标须复位"
        );
    }

    /// DK-42 DoD2: 错误三分类——deadline trap 报 "plugin deadline exceeded"
    /// （宿主可程序化区分插件超时 vs 插件 bug）。
    #[tokio::test]
    async fn dk42_error_classification_deadline() {
        let dir = tempfile::tempdir().unwrap();
        let wasm = wat::parse_str(
            r#"
            (module
                (func (export "spin") (result i32) (loop (br 0)) unreachable)
            )
        "#,
        )
        .unwrap();
        let entry = dir.path().join("spin42.wasm");
        std::fs::write(&entry, wasm).unwrap();

        let mut m = manifest(entry.to_string_lossy().to_string());
        m.sandbox.epoch_deadline_ticks = 20; // 200ms，manifest 级覆盖
        let mut rt = WasmtimeRuntime::new().unwrap();
        let handle = rt.load(&m).await.unwrap();

        let err = rt
            .invoke(&handle, "spin", &serde_json::Value::Null)
            .await
            .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("plugin deadline exceeded"),
            "须分类为 deadline 超期, got {msg}"
        );
        assert!(!msg.contains("wasm call:"), "不得落在通用包装: {msg}");
    }

    /// DK-42 DoD3: serde default 兼容——旧清单（无 sandbox 字段）反序列化
    /// 自动填缺省安全值（2s/64MiB），已上线清单零迁移。
    #[test]
    fn dk42_sandbox_serde_default_compat() {
        let legacy = r#"{"id":"x","name":"x","version":"0","author":"a","description":"d","runtime":"Wasm","entry":"e.wasm","permissions":[],"hooks":[],"block_types":[],"config_schema":null}"#;
        let m: PluginManifest = serde_json::from_str(legacy).unwrap();
        assert_eq!(m.sandbox, SandboxLimits::default());
        assert_eq!(m.sandbox.epoch_deadline_ticks, DEFAULT_DEADLINE_TICKS);
        assert_eq!(m.sandbox.max_memory_bytes, DEFAULT_MAX_MEMORY_BYTES);
    }
}
