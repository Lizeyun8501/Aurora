//! DK-模块编译缓存 engine_tag 加固：从 workspace Cargo.lock 提取实际
//! 锁定的 wasmtime 版本，导出 `WASMTIME_LOCK_VERSION` 编译期环境变量——
//! `cache.rs` 将其拼进 ENGINE_TAG。wasmtime 升级（lock 变更）→ tag 自动
//! 随动 → 旧缓存 miss → fail-open 重编译（零纪律依赖，取代"升级需手动
//! bump tag"的流程保证）。
//!
//! 解析逻辑在 `src/cache_lock.rs`（build.rs 与 lib 单测共享——`#[path]`
//! include）。
//!
//! fail-closed：lock 中找不到与 workspace req（"36"）前缀兼容的 wasmtime
//! 条目 → panic（不可达状态——wasmtime 是硬依赖，lock 异常即编译失败）。

#[path = "src/cache_lock.rs"]
mod cache_lock;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    // lock 变更（含 wasmtime 版本漂移）→ 本 build script 重跑 → tag 随动
    println!("cargo:rerun-if-changed=../../Cargo.lock");

    let manifest_dir =
        std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let lock_path = manifest_dir.join("../../Cargo.lock");
    let lock = std::fs::read_to_string(&lock_path)
        .unwrap_or_else(|e| panic!("read workspace Cargo.lock at {lock_path:?}: {e}"));

    let version = cache_lock::extract_wasmtime_version(&lock, "36.")
        .unwrap_or_else(|| panic!("Cargo.lock: no wasmtime entry matching req prefix '36.'"));

    println!("cargo:rustc-env=WASMTIME_LOCK_VERSION={version}");
}
