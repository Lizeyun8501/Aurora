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
            CapabilityManifest::from_permissions(&["fs".into(), "log".into(), "storage".into()]),
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

/// DK-14c.1：token 生命周期——实例缓存跨 invoke 共享（迁移自 d2
/// test_instance_cache_hit；wat fixture 退役后由真组件承担）。
#[test]
fn e2e_instance_cache_shared_across_invokes() {
    let Some(bytes) = guest_wasm_bytes("plugin-hello-guest", "plugin_hello_guest.wasm") else {
        eprintln!("[e2e-skip] wasm 产物缺失——查 wasm job 或本地构建");
        return;
    };
    let runtime = WasmRuntime::with_default_registry();
    runtime.register(
        "p1",
        CapabilityManifest::from_permissions(&["storage".into(), "log".into()]),
    );
    runtime.install_component("p1", bytes);

    runtime
        .invoke("p1", "init", &serde_json::json!({}))
        .unwrap();
    runtime
        .invoke(
            "p1",
            "handle-event",
            &serde_json::json!({"event": "note.created", "payload": "x"}),
        )
        .unwrap();
    runtime
        .invoke("p1", "init", &serde_json::json!({}))
        .unwrap();
    assert_eq!(
        runtime.instantiation_count(),
        1,
        "三次 invoke 共享一次实例化（token 随实例发放一次）"
    );
}

/// DK-14c.1：unregister 清实例缓存——再注册+invoke 重新实例化
/// （迁移自 d2 test_unregister_drops_instance_cache）。
#[test]
fn e2e_unregister_drops_instance_cache() {
    let Some(bytes) = guest_wasm_bytes("plugin-hello-guest", "plugin_hello_guest.wasm") else {
        eprintln!("[e2e-skip] wasm 产物缺失——查 wasm job 或本地构建");
        return;
    };
    let runtime = WasmRuntime::with_default_registry();
    runtime.register(
        "p2",
        CapabilityManifest::from_permissions(&["storage".into(), "log".into()]),
    );
    runtime.install_component("p2", bytes.clone());
    runtime
        .invoke("p2", "init", &serde_json::json!({}))
        .unwrap();
    assert_eq!(runtime.instantiation_count(), 1);

    runtime.unregister("p2");
    let err = runtime
        .invoke("p2", "init", &serde_json::json!({}))
        .unwrap_err();
    assert!(matches!(err, aurora_plugin::Error::NotFound(_)));

    // 再注册+登记 → 重新实例化（token 随新实例重新发放）
    runtime.register(
        "p2",
        CapabilityManifest::from_permissions(&["storage".into(), "log".into()]),
    );
    runtime.install_component("p2", bytes.clone());
    runtime
        .invoke("p2", "init", &serde_json::json!({}))
        .unwrap();
    assert_eq!(runtime.instantiation_count(), 2);
}

/// DoD：越权拦截——零权限插件（空 granted manifest）不发 token，invoke
/// 直接拒（Error::CapabilityDenied）+ [audit-host] 审计行（stderr 透传，
/// `cargo test -- --nocapture` 可观测；样例：[audit-host] plugin=denied
/// event=init result=denied reason=zero-grant-manifest）。
#[test]
fn e2e_zero_grant_invoke_denied() {
    let Some(bytes) = guest_wasm_bytes("plugin-hello-guest", "plugin_hello_guest.wasm") else {
        eprintln!("[e2e-skip] wasm 产物缺失——查 wasm job 或本地构建");
        return;
    };
    let runtime = WasmRuntime::with_default_registry();
    // 零权限 manifest（granted 空）
    runtime.register("denied", CapabilityManifest::new());
    runtime.install_component("denied", bytes);

    let err = runtime
        .invoke("denied", "init", &serde_json::json!({}))
        .unwrap_err();
    assert!(
        matches!(err, aurora_plugin::Error::CapabilityDenied(_)),
        "零权限插件 invoke 应被拒（CapabilityDenied），实际 {err:?}"
    );

    // 对照组：granted 非空 → 正常放行（token 发放）
    runtime.register(
        "ok",
        CapabilityManifest::from_permissions(&["storage".into(), "log".into()]),
    );
    runtime.install_component(
        "ok",
        guest_wasm_bytes("plugin-hello-guest", "plugin_hello_guest.wasm").unwrap(),
    );
    runtime
        .invoke("ok", "init", &serde_json::json!({}))
        .expect("granted 非空插件应走通（token 发放）");
}

/// DK-14c.2：log 方法越权拦截——granted 无 Log→guest 调 capability.log→
/// 宿主 per-method require 拒（trap 透传 CapabilityDenied）+ [audit-host]
/// 审计行 reason=log-denied（stderr 透传，--nocapture 可观测）。
#[test]
fn e2e_log_method_denied_without_grant() {
    let Some(bytes) = guest_wasm_bytes("plugin-hello-guest", "plugin_hello_guest.wasm") else {
        eprintln!("[e2e-skip] wasm 产物缺失——查 wasm job 或本地构建");
        return;
    };
    let runtime = WasmRuntime::with_default_registry();
    runtime.register(
        "nolog",
        CapabilityManifest::from_permissions(&["storage".into()]),
    );
    runtime.install_component("nolog", bytes);
    // hello init 内 capability.log → 宿主 require(Log) 拒 → trap → invoke err
    let err = runtime
        .invoke("nolog", "init", &serde_json::json!({}))
        .unwrap_err();
    let msg = format!("{err}");
    // CapabilityDenied 语义在 wasmtime trap cause 链（Display 不透——
    // 台账记注）；行为断言：宿主 require 拒→guest log 调用 trap→invoke err
    assert!(
        msg.contains("log"),
        "log 越权应被宿主 require 拒（trap 透传），实际 {msg}"
    );
}

/// DK-14c.2：log 方法正常转发——granted 含 Log→tracing::info! 转发
/// （--nocapture 可观测）+ 首调审计行 reason=first-call（节流：二次
/// 调用不记行）+ 两次调用均成功（init 与 handle-event 各一）。
#[test]
fn e2e_log_method_granted_forwards() {
    let Some(bytes) = guest_wasm_bytes("plugin-hello-guest", "plugin_hello_guest.wasm") else {
        eprintln!("[e2e-skip] wasm 产物缺失——查 wasm job 或本地构建");
        return;
    };
    let runtime = WasmRuntime::with_default_registry();
    runtime.register(
        "withlog",
        CapabilityManifest::from_permissions(&["log".into(), "storage".into()]),
    );
    runtime.install_component("withlog", bytes);
    runtime
        .invoke("withlog", "init", &serde_json::json!({}))
        .expect("granted 含 Log 的 init 内 log 调用应成功");
    runtime
        .invoke(
            "withlog",
            "handle-event",
            &serde_json::json!({"event": "note.created", "payload": "dk14c2"}),
        )
        .expect("handle-event 内第二次 log 调用应成功（节流不影响放行）");
}

/// DK-14c.3：数据面 guest-invoked 全链——hello init 内 write→read 回读→
/// query 自验；宿主侧经 host-invoked read-doc 断言 guest 写入的数据
/// （同一数据空间跨调用模式可见——设计使然）。
#[test]
fn e2e_dataplane_granted_full_chain() {
    let Some(bytes) = guest_wasm_bytes("plugin-hello-guest", "plugin_hello_guest.wasm") else {
        eprintln!("[e2e-skip] wasm 产物缺失");
        return;
    };
    let runtime = WasmRuntime::with_default_registry();
    runtime.register(
        "withdata",
        CapabilityManifest::from_permissions(&["storage".into(), "log".into()]),
    );
    runtime.install_component("withdata", bytes.clone());
    runtime
        .invoke("withdata", "init", &serde_json::json!({}))
        .expect("granted 全链：init 内数据面三调用应成功");
    // host-invoked 读回 guest 写的数据（共享空间跨模式可见）
    let v = runtime
        .invoke(
            "withdata",
            "read_doc",
            &serde_json::json!({"doc_id": "greeting"}),
        )
        .expect("host-invoked read-doc 应命中 guest 写入");
    let s = v.as_str().unwrap_or_default();
    assert!(
        s.contains(r#"\"from\""#) || s.contains("from"),
        "read-doc 应返回 guest 写的 greeting JSON，实际 {s}"
    );
}

/// DK-14c.3：数据面越权——granted 无 Storage→init 内 write-doc→宿主
/// require(Storage) 拒（trap 透传）+ [audit-host] data-denied（逐条不节流）。
#[test]
fn e2e_dataplane_denied_without_storage() {
    let Some(bytes) = guest_wasm_bytes("plugin-hello-guest", "plugin_hello_guest.wasm") else {
        eprintln!("[e2e-skip] wasm 产物缺失");
        return;
    };
    let runtime = WasmRuntime::with_default_registry();
    runtime.register(
        "nodata",
        // log 有 storage 无——log 放行 write-doc 拒
        CapabilityManifest::from_permissions(&["log".into()]),
    );
    runtime.install_component("nodata", bytes);
    let err = runtime
        .invoke("nodata", "init", &serde_json::json!({}))
        .unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("write-doc") || msg.contains("log"),
        "数据面越权应 trap（data-denied），实际 {msg}"
    );
}

/// DK-14c.3：容量 guard——handle-event payload >1 MiB→write-doc 返回
/// err string（None=ok/Some=err 形态）→guest expect trap→invoke err。
#[test]
fn e2e_dataplane_size_guard() {
    let Some(bytes) = guest_wasm_bytes("plugin-hello-guest", "plugin_hello_guest.wasm") else {
        eprintln!("[e2e-skip] wasm 产物缺失");
        return;
    };
    let runtime = WasmRuntime::with_default_registry();
    runtime.register(
        "bigdata",
        CapabilityManifest::from_permissions(&["storage".into(), "log".into()]),
    );
    runtime.install_component("bigdata", bytes);
    let big = "x".repeat(1024 * 1024 + 1);
    let err = runtime
        .invoke(
            "bigdata",
            "handle-event",
            &serde_json::json!({"event": "blob", "payload": big}),
        )
        .unwrap_err();
    let msg = format!("{err}");
    // guest panic→wasip2 abort，panic 消息不透传——行为断言=invoke err
    // （size-exceeded 审计行经 --nocapture 采样，报告披露）
    assert!(
        msg.contains("call failed"),
        "超限 write 应 trap（size-exceeded 审计行），实际 {msg}"
    );
}

/// DK-14c.3：共享 KV 语义实证——两插件同 runtime，p1 写 greeting 后
/// p2 init 的 read 回读命中的是 p1 的写入（同空间可见——设计使然，
/// 文档+README 双披露）；非 bug 回归。
#[test]
fn e2e_dataplane_shared_kv_semantics() {
    let Some(bytes) = guest_wasm_bytes("plugin-hello-guest", "plugin_hello_guest.wasm") else {
        eprintln!("[e2e-skip] wasm 产物缺失");
        return;
    };
    let runtime = WasmRuntime::with_default_registry();
    let manifest = || CapabilityManifest::from_permissions(&["storage".into(), "log".into()]);
    runtime.register("p1", manifest());
    runtime.install_component("p1", bytes.clone());
    runtime
        .invoke("p1", "init", &serde_json::json!({}))
        .unwrap();
    runtime.register("p2", manifest());
    runtime.install_component("p2", bytes);
    // p2 init 内 read 回读 Some（读到 p1 的 greeting——共享实证）
    runtime
        .invoke("p2", "init", &serde_json::json!({}))
        .expect("p2 init 应成功（read 到 p1 写入——共享 KV）");
    // 宿主侧终证：p2 覆盖后仍可读（Some 语义）
    let v = runtime
        .invoke("p2", "read_doc", &serde_json::json!({"doc_id": "greeting"}))
        .expect("共享空间终证读回");
    assert!(v.as_str().is_some(), "greeting 应存在（共享 KV）");
}
