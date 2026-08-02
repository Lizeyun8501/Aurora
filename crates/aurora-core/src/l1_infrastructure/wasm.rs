//! WASM 运行时 (基于 Wasmtime)
//!
//! 提供安全沙箱化的 WebAssembly 运行时能力，用于隔离执行插件代码。
//! 底层使用 [Wasmtime](https://wasmtime.dev/) 实现。

/// WASM 运行时引擎占位类型。
///
/// 实际实现将在后续任务中封装 Wasmtime 的 `Engine` 与 `Store` 等能力。
pub struct WasmRuntime;
