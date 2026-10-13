//! DK-14e.1 端到端集成证明：宿主加载**真 wasm 产物**（wasm32-wasip2 组件）
//! 跑 install_component → register → invoke 全链。
//!
//! 超越 d2 的 wat fixture：真 wasip2 组件（wasm-tools 编译产物、含 WASI
//! 依赖面）被 wasmtime 36 实例化并完成 init/handle-event 调用——插件生态
//! 宿主/guest 双面齐备的最终证明。
//!
//! 容忍设计（工单口径）：wasm 产物缺失时跳过（提示构建命令）——CI wasm
//! job 或本机构建产物均可；产物不入库（卫生红线），测试按需现场构建。

use std::path::PathBuf;
use std::process::Command;

use aurora_plugin::{CapabilityManifest, WasmRuntime};

/// 三示范插件（CI wasm-guest 矩阵同款）：crate 名与 wasm 产物文件名。
const GUESTS: &[(&str, &str)] = &[
    ("plugin-hello-guest", "plugin_hello_guest.wasm"),
    ("plugin-wordcount-guest", "plugin_wordcount_guest.wasm"),
    ("plugin-auditlog-guest", "plugin_auditlog_guest.wasm"),
];

/// 取真 wasm 产物字节：优先已构建产物，缺失则现场构建（release）。
fn guest_wasm_bytes(crate_name: &str, wasm_file: &str) -> Option<Vec<u8>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../target/wasm32-wasip2/release/{wasm_file}"));
    if !path.exists() {
        // 现场构建（产物缺失容忍场景——构建失败则 skip）
        let rc = Command::new("cargo")
            .args([
                "build",
                "-p",
                crate_name,
                "--target",
                "wasm32-wasip2",
                "--release",
            ])
            .status()
            .ok()?;
        if !rc.success() {
            eprintln!(
                "[e2e-skip] {crate_name} wasm build failed——产物缺失且现场构建失败；\
                 提示：cargo build -p {crate_name} --target wasm32-wasip2 --release"
            );
            return None;
        }
    }
    std::fs::read(&path)
        .map_err(|e| eprintln!("[e2e-skip] {crate_name} wasm read failed: {e}"))
        .ok()
}

/// 真组件全链：install → register → init → handle-event。
///
/// 断言口径（行为级）：真 wasip2 组件被 wasmtime 36 **实例化**并完成
/// **两次导出调用**（init/handle-event 返回 Ok）——stderr 侧信道输出
/// 由 wasmtime 透传宿主 stderr（CI 日志可观测；前缀约定见
/// docs/plugin-log-conventions.md）。
#[test]
fn e2e_real_wasip2_components_full_chain() {
    let runtime = WasmRuntime::with_default_registry();
    let mut installed = 0;

    for (crate_name, wasm_file) in GUESTS {
        let plugin_id = crate_name.trim_end_matches("-guest");
        let Some(bytes) = guest_wasm_bytes(crate_name, wasm_file) else {
            eprintln!(
                "[e2e-skip] {plugin_id}: wasm 产物缺失（CI wasm job 产物或本地 \
                 `cargo build -p {crate_name} --target wasm32-wasip2 --release` 后重跑）"
            );
            continue;
        };

        // 1) install：真 wasm 字节登记（非 wat fixture）
        runtime.install_component(plugin_id, bytes);

        // 2) register：capability 串由 Manifest granted 集合派生（d2 合约）
        runtime.register(
            plugin_id,
            CapabilityManifest::from_permissions(&["fs".into()]),
        );

        // 3) invoke：init + handle-event 真调用（真组件被 wasmtime 36
        //    实例化并执行——本测试的核心断言；stderr 侧信道可观测但
        //    宿主不捕获——wasmtime 默认透传）
        runtime
            .invoke(plugin_id, "init", &serde_json::json!({}))
            .unwrap_or_else(|e| panic!("{plugin_id} init invoke failed: {e}"));
        runtime
            .invoke(
                plugin_id,
                "handle-event",
                &serde_json::json!({
                    "event": "note.created",
                    "payload": "{\"text\":\"a b c\",\"id\":7}"
                }),
            )
            .unwrap_or_else(|e| panic!("{plugin_id} handle-event invoke failed: {e}"));

        installed += 1;
        eprintln!("[e2e] {plugin_id}: real wasip2 component full chain OK");
    }

    assert_eq!(
        installed, 3,
        "三示范插件应全部走通真组件全链（缺失即 skip/报错——查 wasm job 或本地构建）"
    );
}
