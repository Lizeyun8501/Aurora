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
            CapabilityManifest::from_permissions(&[
                "fs".into(),
                "log".into(),
                "storage".into(),
                "user-output".into(),
            ])
            .unwrap(),
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
        "plugin-hello",
        CapabilityManifest::from_permissions(&[
            "storage".into(),
            "log".into(),
            "user-output".into(),
        ])
        .unwrap(),
    );
    runtime.install_component("plugin-hello", bytes);

    runtime
        .invoke("plugin-hello", "init", &serde_json::json!({}))
        .unwrap();
    runtime
        .invoke(
            "plugin-hello",
            "handle-event",
            &serde_json::json!({"event": "note.created", "payload": "x"}),
        )
        .unwrap();
    runtime
        .invoke("plugin-hello", "init", &serde_json::json!({}))
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
        "plugin-hello",
        CapabilityManifest::from_permissions(&[
            "storage".into(),
            "log".into(),
            "user-output".into(),
        ])
        .unwrap(),
    );
    runtime.install_component("plugin-hello", bytes.clone());
    runtime
        .invoke("plugin-hello", "init", &serde_json::json!({}))
        .unwrap();
    assert_eq!(runtime.instantiation_count(), 1);

    runtime.unregister("plugin-hello");
    let err = runtime
        .invoke("plugin-hello", "init", &serde_json::json!({}))
        .unwrap_err();
    assert!(matches!(err, aurora_plugin::Error::NotFound(_)));

    // 再注册+登记 → 重新实例化（token 随新实例重新发放）
    runtime.register(
        "plugin-hello",
        CapabilityManifest::from_permissions(&[
            "storage".into(),
            "log".into(),
            "user-output".into(),
        ])
        .unwrap(),
    );
    runtime.install_component("plugin-hello", bytes.clone());
    runtime
        .invoke("plugin-hello", "init", &serde_json::json!({}))
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
        "plugin-hello",
        CapabilityManifest::from_permissions(&[
            "storage".into(),
            "log".into(),
            "user-output".into(),
        ])
        .unwrap(),
    );
    runtime.install_component(
        "plugin-hello",
        guest_wasm_bytes("plugin-hello-guest", "plugin_hello_guest.wasm").unwrap(),
    );
    runtime
        .invoke("plugin-hello", "init", &serde_json::json!({}))
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
        CapabilityManifest::from_permissions(&["storage".into()]).unwrap(),
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
        "plugin-hello",
        CapabilityManifest::from_permissions(&[
            "log".into(),
            "storage".into(),
            "user-output".into(),
        ])
        .unwrap(),
    );
    runtime.install_component("plugin-hello", bytes);
    runtime
        .invoke("plugin-hello", "init", &serde_json::json!({}))
        .expect("granted 含 Log 的 init 内 log 调用应成功");
    runtime
        .invoke(
            "plugin-hello",
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
        "plugin-hello",
        CapabilityManifest::from_permissions(&[
            "storage".into(),
            "log".into(),
            "user-output".into(),
        ])
        .unwrap(),
    );
    runtime.install_component("plugin-hello", bytes.clone());
    runtime
        .invoke("plugin-hello", "init", &serde_json::json!({}))
        .expect("granted 全链：init 内数据面三调用应成功");
    // host-invoked 读回 guest 写的数据（共享空间跨模式可见）
    let v = runtime
        .invoke(
            "plugin-hello",
            "read_doc",
            &serde_json::json!({"doc_id": "plugin-hello:greeting"}),
        )
        .expect("host-invoked read-doc 应命中 guest 写入");
    // DK-14c.4：类型断言（哨兵协议撤）——read-doc 返回 option<string>
    // →host-invoked 读回 Value::String(json)→解析断言字段
    let s = v.as_str().unwrap_or_default();
    let parsed: serde_json::Value =
        serde_json::from_str(s).expect("guest 写的 greeting 应为合法 JSON");
    assert_eq!(
        parsed.get("from").and_then(|f| f.as_str()),
        Some("hello"),
        "greeting.from 应为 hello（类型断言），实际 {parsed}"
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
        CapabilityManifest::from_permissions(&["log".into()]).unwrap(),
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
        "plugin-hello",
        CapabilityManifest::from_permissions(&[
            "storage".into(),
            "log".into(),
            "user-output".into(),
        ])
        .unwrap(),
    );
    runtime.install_component("plugin-hello", bytes);
    let big = "x".repeat(1024 * 1024 + 1);
    let err = runtime
        .invoke(
            "plugin-hello",
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

/// DK-16 L1 隔离语义（取代 14c.3 共享 KV——namespace 默认化后跨前缀
/// 互不可见；同 hello wasm 双 id：p2 init 的 read 回读 None（p1 数据
/// 对 p2 不可见）；host 特权面终证 p1 数据仍在。
#[test]
fn e2e_dataplane_namespace_isolation() {
    let Some(bytes) = guest_wasm_bytes("plugin-hello-guest", "plugin_hello_guest.wasm") else {
        eprintln!("[e2e-skip] wasm 产物缺失");
        return;
    };
    let runtime = WasmRuntime::with_default_registry();
    // plugin-hello：前缀与 hello 写死前缀对齐（全链过）
    runtime.register(
        "plugin-hello",
        CapabilityManifest::from_permissions(&[
            "storage".into(),
            "log".into(),
            "user-output".into(),
        ])
        .unwrap(),
    );
    runtime.install_component("plugin-hello", bytes.clone());
    runtime
        .invoke("plugin-hello", "init", &serde_json::json!({}))
        .expect("plugin-hello init 应过（前缀匹配）");
    // p2：同 wasm 不同 id——前缀 "p2:"≠"plugin-hello:"，init read 即拒
    runtime.register(
        "p2",
        CapabilityManifest::from_permissions(&[
            "storage".into(),
            "log".into(),
            "user-output".into(),
        ])
        .unwrap(),
    );
    runtime.install_component("p2", bytes);
    // A-5 后 p2 init 自包含（SELF_ID 派生——读写 p2:* 不跨前缀）
    runtime
        .invoke("p2", "init", &serde_json::json!({}))
        .expect("p2 init 应过（A-5 self-id 派生——自包含语义）");
    // 隔离断言（guest 面 read-back 探针）：p2 读 A 的键仍拒
    let r = runtime.invoke(
        "p2",
        "handle-event",
        &serde_json::json!({"event": "read-back", "payload": "plugin-hello:greeting"}),
    );
    assert!(
        r.is_err(),
        "p2 跨前缀 read 应拒（namespace 隔离），实际 {r:?}"
    );
    // host 特权面终证：p1 数据仍在（隔离≠删除）
    let v = runtime
        .invoke(
            "plugin-hello",
            "read_doc",
            &serde_json::json!({"doc_id": "plugin-hello:greeting"}),
        )
        .expect("host 读回");
    assert!(
        v.as_str().is_some(),
        "plugin-hello:greeting 应存在（隔离不删数据）"
    );
}

/// DK-14c.4 回归例：空内容文档（option 语义——空串可表示，非哨兵歧义）
/// 与含换行 doc_id（list 键集类型保证）。断言走 host-invoked 读回和
/// query 键集（类型断言）。
#[test]
fn e2e_dataplane_empty_content_and_newline_key() {
    let Some(bytes) = guest_wasm_bytes("plugin-hello-guest", "plugin_hello_guest.wasm") else {
        eprintln!("[e2e-skip] wasm 产物缺失");
        return;
    };
    let runtime = WasmRuntime::with_default_registry();
    runtime.register(
        "plugin-hello",
        CapabilityManifest::from_permissions(&[
            "storage".into(),
            "log".into(),
            "user-output".into(),
        ])
        .unwrap(),
    );
    runtime.install_component("plugin-hello", bytes);
    // guest init 内：write("note-empty","")→read Some("") 自验；
    // write("line\nbreak","nl")→query("line\n") 含换行键自验
    runtime
        .invoke("plugin-hello", "init", &serde_json::json!({}))
        .expect("回归自验应过（空内容+含换行 doc_id）");
    // host-invoked 终证：空内容可表示
    let v = runtime
        .invoke(
            "plugin-hello",
            "read_doc",
            &serde_json::json!({"doc_id": "plugin-hello:note-empty"}),
        )
        .expect("note-empty 应存在");
    assert_eq!(v.as_str(), Some(""), "空内容文档应可表示（Some(\"\")）");
    // host-invoked query 终证：含换行键完整（list 无分隔歧义）
    let q = runtime
        .invoke(
            "plugin-hello",
            "query",
            &serde_json::json!({"query": "line\n"}),
        )
        .expect("query 应返回键集");
    let keys = q
        .get("results")
        .and_then(|r| r.as_array())
        .expect("键集应为数组（d2 query 结果形态）");
    assert!(
        keys.iter()
            .any(|k| k.as_str().map(|s| s.contains('\n')).unwrap_or(false)),
        "键集应含换行键，实际 {keys:?}"
    );
}

/// DK-15b e2e：用户可见输出受控通道——granted 打印可拉取（拉取即消费：
/// 二次拉取为空）。
#[test]
fn e2e_useroutput_pull() {
    let Some(bytes) = guest_wasm_bytes("plugin-hello-guest", "plugin_hello_guest.wasm") else {
        eprintln!("[e2e-skip] wasm 产物缺失");
        return;
    };
    let runtime = WasmRuntime::with_default_registry();
    runtime.register(
        "plugin-hello",
        CapabilityManifest::from_permissions(&[
            "storage".into(),
            "log".into(),
            "user-output".into(),
        ])
        .unwrap(),
    );
    runtime.install_component("plugin-hello", bytes);
    runtime
        .invoke("plugin-hello", "init", &serde_json::json!({}))
        .expect("init 应过（含 data 面自验）");
    runtime
        .invoke(
            "plugin-hello",
            "handle-event",
            &serde_json::json!({"event": "pull", "payload": "x"}),
        )
        .expect("handle-event 应过（print granted）");
    let v = runtime
        .invoke("plugin-hello", "pull_output", &serde_json::json!({}))
        .expect("pull_output 应返回缓冲行");
    let lines = v
        .get("lines")
        .and_then(|l| l.as_array())
        .expect("lines 应为数组");
    assert!(
        lines.iter().any(|l| l
            .as_str()
            .map(|s| s.contains("hello event=pull"))
            .unwrap_or(false)),
        "缓冲应含 print 行，实际 {lines:?}"
    );
    // 拉取即消费：二次拉取为空
    let v2 = runtime
        .invoke("plugin-hello", "pull_output", &serde_json::json!({}))
        .expect("二次拉取应过");
    let lines2 = v2
        .get("lines")
        .and_then(|l| l.as_array())
        .expect("lines 应为数组");
    assert!(
        lines2.is_empty(),
        "拉取即消费——二次拉取应为空，实际 {lines2:?}"
    );
}

/// DK-15b e2e：越权拒——manifest 无 user-output→print panic（trap）
/// →handle-event invoke 报 Err（差异化审计 denied 留痕）。
#[test]
fn e2e_useroutput_denied() {
    let Some(bytes) = guest_wasm_bytes("plugin-hello-guest", "plugin_hello_guest.wasm") else {
        eprintln!("[e2e-skip] wasm 产物缺失");
        return;
    };
    let runtime = WasmRuntime::with_default_registry();
    runtime.register(
        "plugin-hello",
        // 窄授权：无 user-output——print 应被 require 拒
        CapabilityManifest::from_permissions(&["storage".into(), "log".into()]).unwrap(),
    );
    runtime.install_component("plugin-hello", bytes);
    let r = runtime.invoke(
        "plugin-hello",
        "handle-event",
        &serde_json::json!({"event": "deny", "payload": "y"}),
    );
    assert!(
        r.is_err(),
        "无 user-output 的插件 print 应 trap 拒绝，实际 {r:?}"
    );
    // 二次 pull 无缓冲（越权调用未达 push）
    let v = runtime
        .invoke("plugin-hello", "pull_output", &serde_json::json!({}))
        .expect("pull_output 面不设权限");
    let lines = v
        .get("lines")
        .and_then(|l| l.as_array())
        .expect("lines 应为数组");
    assert!(lines.is_empty(), "越权 print 不得入缓冲");
}

/// DK-16 L1：namespace 强制前缀——同 guest 双 id 模拟：p2（无 "plugin-hello"
/// 前缀授权）read 他前缀键→denied（init 内 read 自验即触发）。
#[test]
fn e2e_namespace_read_denied() {
    let Some(bytes) = guest_wasm_bytes("plugin-hello-guest", "plugin_hello_guest.wasm") else {
        eprintln!("[e2e-skip] wasm 产物缺失");
        return;
    };
    let runtime = WasmRuntime::with_default_registry();
    runtime.register(
        "p2",
        CapabilityManifest::from_permissions(&[
            "storage".into(),
            "log".into(),
            "user-output".into(),
        ])
        .unwrap(),
    );
    runtime.install_component("p2", bytes);
    // A-5 后 p2 init 自包含（SELF_ID 派生——init 读写 p2:* 应过）
    runtime
        .invoke("p2", "init", &serde_json::json!({}))
        .expect("p2 init 应过（A-5 self-id 派生——写死前缀退役）");
    // 跨前缀 read 拒断言移至 read-back 探针（guest 面）
    let r = runtime.invoke(
        "p2",
        "handle-event",
        &serde_json::json!({"event": "read-back", "payload": "plugin-hello:greeting"}),
    );
    assert!(r.is_err(), "跨前缀 read 应 namespace 拒，实际 {r:?}");
}

/// DK-16 L1：namespace 强制前缀——write 拒（p2 handle-event 首动作
/// write "plugin-hello:event-..."）。
#[test]
fn e2e_namespace_write_denied() {
    let Some(bytes) = guest_wasm_bytes("plugin-hello-guest", "plugin_hello_guest.wasm") else {
        eprintln!("[e2e-skip] wasm 产物缺失");
        return;
    };
    let runtime = WasmRuntime::with_default_registry();
    runtime.register(
        "p2",
        CapabilityManifest::from_permissions(&[
            "storage".into(),
            "log".into(),
            "user-output".into(),
        ])
        .unwrap(),
    );
    runtime.install_component("p2", bytes);
    let r = runtime.invoke(
        "p2",
        "handle-event",
        &serde_json::json!({"event": "cross", "payload": "x"}),
    );
    assert!(r.is_err(), "跨前缀 write 应 namespace 拒，实际 {r:?}");
}

/// DK-16 L1：query 键集本前缀过滤——host 预置他前缀键（特权面），
/// p1 query 全集不得含跨前缀键（防枚举探测）。
#[test]
fn e2e_query_isolation() {
    let Some(bytes) = guest_wasm_bytes("plugin-hello-guest", "plugin_hello_guest.wasm") else {
        eprintln!("[e2e-skip] wasm 产物缺失");
        return;
    };
    let runtime = WasmRuntime::with_default_registry();
    runtime.register(
        "plugin-hello",
        CapabilityManifest::from_permissions(&[
            "storage".into(),
            "log".into(),
            "user-output".into(),
        ])
        .unwrap(),
    );
    runtime.install_component("plugin-hello", bytes);
    runtime
        .invoke("plugin-hello", "init", &serde_json::json!({}))
        .expect("init 应过（SELF_ID 派生源）");
    // host 特权面预置跨前缀键（模拟他插件存量数据——host 不过 guest guard）
    runtime
        .invoke(
            "plugin-hello",
            "write_doc",
            &serde_json::json!({"doc_id": "other-plugin:secret", "data": "1"}),
        )
        .expect("host 特权面写跨前缀键应放行");
    runtime
        .invoke(
            "plugin-hello",
            "handle-event",
            &serde_json::json!({"event": "q", "payload": "y"}),
        )
        .expect("handle-event 应过（前缀合法）");
    let v = runtime
        .invoke(
            "plugin-hello",
            "read_doc",
            &serde_json::json!({"doc_id": "plugin-hello:query-keys"}),
        )
        .expect("query-keys 应可读");
    let keys = v.as_str().unwrap_or_default();
    assert!(
        keys.contains("plugin-hello:event-q"),
        "本前缀键应在 query 集，实际 {keys:?}"
    );
    assert!(
        !keys.contains("other-plugin:"),
        "跨前缀键不得出现在本插件 query 集（隔离/防枚举），实际 {keys:?}"
    );
}

/// DK-16 L1：storage_methods 声明后越 method 拒（渐进授权——缺省全开
/// 由 full_chain 存量 manifest 零改动全链过回归证明）。
#[test]
fn e2e_storage_method_denied() {
    let Some(bytes) = guest_wasm_bytes("plugin-hello-guest", "plugin_hello_guest.wasm") else {
        eprintln!("[e2e-skip] wasm 产物缺失");
        return;
    };
    let runtime = WasmRuntime::with_default_registry();
    runtime.register(
        "m1",
        CapabilityManifest::from_permissions_with_methods(
            &["storage".into(), "log".into(), "user-output".into()],
            &["read".into()],
        )
        .expect("合法 method 集应过"),
    );
    runtime.install_component("m1", bytes);
    // 声明仅 read——handle-event 的 write_doc 应 method 拒
    let r = runtime.invoke(
        "m1",
        "handle-event",
        &serde_json::json!({"event": "mw", "payload": "z"}),
    );
    assert!(
        r.is_err(),
        "越 method write 应拒（声明仅 read），实际 {r:?}"
    );
    // read 仍在授权集——init 全链过（write 不经 init？init 含 write→拒，
    // 故用 host 读路径验证 read 面；init 亦应因 write 拒而 Err）
    let init = runtime.invoke("m1", "init", &serde_json::json!({}));
    assert!(
        init.is_err(),
        "init 含 write——声明仅 read 时应拒，实际 {init:?}"
    );
}

/// C-15a-test 清账：from_permissions 未知 capability 串直接拒（含非法串+
/// position+合法清单断言——15a fail-closed 主路径直接单测）。
#[test]
fn unknown_permission_rejected() {
    let Err(err) = CapabilityManifest::from_permissions(&["fs".into(), "bogus-perm".into()]) else {
        panic!("未知串必须拒绝加载");
    };
    let msg = err.to_string();
    assert!(msg.contains("bogus-perm"), "诊断应含非法串，实际 {msg:?}");
    assert!(
        msg.contains("position 1"),
        "诊断应含 position，实际 {msg:?}"
    );
    assert!(msg.contains("valid set"), "诊断应含合法清单，实际 {msg:?}");
    // L1 搭车：未知 storage method 串同 fail-closed
    let Err(merr) = CapabilityManifest::from_permissions_with_methods(
        &["storage".into()],
        &["bogus-method".into()],
    ) else {
        panic!("未知 storage method 必须拒绝");
    };
    let mmsg = merr.to_string();
    assert!(
        mmsg.contains("bogus-method"),
        "诊断应含非法 method，实际 {mmsg:?}"
    );
    assert!(
        mmsg.contains("position 0"),
        "诊断应含 position，实际 {mmsg:?}"
    );
}

/// DK-17 L2：share 只读授权——A 写→host share→B（accept_share opt-in）
/// 读命中；未 share 键 B 读拒；B 写 A 空间拒（share 只读）。
#[test]
fn e2e_share_read_and_isolation() {
    let Some(bytes) = guest_wasm_bytes("plugin-hello-guest", "plugin_hello_guest.wasm") else {
        eprintln!("[e2e-skip] wasm 产物缺失");
        return;
    };
    let runtime = WasmRuntime::with_default_registry();
    // A=plugin-hello（hello 前缀派生自 self-id）
    let mut ma = CapabilityManifest::from_permissions(&[
        "storage".into(),
        "log".into(),
        "user-output".into(),
    ])
    .unwrap();
    ma.accept_share = false; // A 不接收分享（write 路径不受影响）
    runtime.register("plugin-hello", ma);
    runtime.install_component("plugin-hello", bytes.clone());
    runtime
        .invoke("plugin-hello", "init", &serde_json::json!({}))
        .expect("A init 应过");
    // B=p2（opt-in 接收分享）
    let mut mb = CapabilityManifest::from_permissions(&[
        "storage".into(),
        "log".into(),
        "user-output".into(),
    ])
    .unwrap();
    mb.accept_share = true;
    runtime.register("p2", mb.clone());
    runtime.install_component("p2", bytes.clone());
    // p2 init 先行（自包含——A-5 self-id 派生，读写 p2:* 不跨前缀）
    runtime
        .invoke("p2", "init", &serde_json::json!({}))
        .expect("p2 init 应过（A-5 self-id 派生）");
    // share：A 的 greeting 键 → p2（宿主特权面）
    let r = runtime.invoke(
        "plugin-hello",
        "share",
        &serde_json::json!({"doc_id": "plugin-hello:greeting", "to_plugin": "p2"}),
    );
    assert!(r.is_ok(), "share 应过（A 键+p2 opt-in），实际 {r:?}");
    // 未 share 键：p2 guest 面读拒（namespace-denied 不变——read-back 探针；
    // trap 后实例不可重入——14d.2 实证，后续断言前 re-install 清缓存）
    let denied = runtime.invoke(
        "p2",
        "handle-event",
        &serde_json::json!({"event": "read-back", "payload": "plugin-hello:note-empty"}),
    );
    assert!(
        denied.is_err(),
        "未 share 键 guest 面读应拒，实际 {denied:?}"
    );
    runtime.install_component("p2", bytes.clone());
    // share 命中：p2 guest 面读 A 的 greeting（share 豁免放行）
    let hit = runtime.invoke(
        "p2",
        "handle-event",
        &serde_json::json!({"event": "read-back", "payload": "plugin-hello:greeting"}),
    );
    assert!(hit.is_ok(), "share 命中应放行 read，实际 {hit:?}");
    // B 写 A 空间拒（写路径不豁免——share 只读；guest 面 write-back 探针）
    let wdenied = runtime.invoke(
        "p2",
        "handle-event",
        &serde_json::json!({"event": "write-back", "payload": "plugin-hello:greeting|hijack"}),
    );
    assert!(
        wdenied.is_err(),
        "B 写 A 空间应拒（share 只读），实际 {wdenied:?}"
    );
}

/// DK-17 L2：accept_share=false 时 share 动作本身拒（fail-closed 默认）。
#[test]
fn e2e_share_target_not_optin_denied() {
    let Some(bytes) = guest_wasm_bytes("plugin-hello-guest", "plugin_hello_guest.wasm") else {
        eprintln!("[e2e-skip] wasm 产物缺失");
        return;
    };
    let runtime = WasmRuntime::with_default_registry();
    runtime.register(
        "plugin-hello",
        CapabilityManifest::from_permissions(&[
            "storage".into(),
            "log".into(),
            "user-output".into(),
        ])
        .unwrap(),
    );
    runtime.install_component("plugin-hello", bytes.clone());
    runtime.register(
        "p3",
        // 缺省 accept_share=false（fail-closed）
        CapabilityManifest::from_permissions(&[
            "storage".into(),
            "log".into(),
            "user-output".into(),
        ])
        .unwrap(),
    );
    runtime.install_component("p3", bytes);
    let r = runtime.invoke(
        "plugin-hello",
        "share",
        &serde_json::json!({"doc_id": "plugin-hello:greeting", "to_plugin": "p3"}),
    );
    assert!(r.is_err(), "目标未 opt-in 的 share 应拒，实际 {r:?}");
}

/// DK-17 L2：revoke 后 B 读拒（授权可撤销）。
#[test]
fn e2e_share_revoke_blocks_read() {
    let Some(bytes) = guest_wasm_bytes("plugin-hello-guest", "plugin_hello_guest.wasm") else {
        eprintln!("[e2e-skip] wasm 产物缺失");
        return;
    };
    let runtime = WasmRuntime::with_default_registry();
    runtime.register(
        "plugin-hello",
        CapabilityManifest::from_permissions(&[
            "storage".into(),
            "log".into(),
            "user-output".into(),
        ])
        .unwrap(),
    );
    runtime.install_component("plugin-hello", bytes.clone());
    let mut mb = CapabilityManifest::from_permissions(&[
        "storage".into(),
        "log".into(),
        "user-output".into(),
    ])
    .unwrap();
    mb.accept_share = true;
    runtime.register("p2", mb.clone());
    runtime.install_component("p2", bytes);
    // A init（先于 share——greeting 键写入 docs，share 对象真实存在）
    runtime
        .invoke("plugin-hello", "init", &serde_json::json!({}))
        .expect("A init 应过");
    runtime
        .invoke(
            "plugin-hello",
            "share",
            &serde_json::json!({"doc_id": "plugin-hello:greeting", "to_plugin": "p2"}),
        )
        .expect("share 应过");
    runtime
        .invoke("p2", "init", &serde_json::json!({}))
        .expect("p2 init 应过");
    let hit = runtime.invoke(
        "p2",
        "handle-event",
        &serde_json::json!({"event": "read-back", "payload": "plugin-hello:greeting"}),
    );
    assert!(hit.is_ok(), "revoke 前 read 应命中，实际 {hit:?}");
    // revoke
    runtime
        .invoke(
            "plugin-hello",
            "revoke",
            &serde_json::json!({"doc_id": "plugin-hello:greeting", "to_plugin": "p2"}),
        )
        .expect("revoke 应过");
    let denied = runtime.invoke(
        "p2",
        "handle-event",
        &serde_json::json!({"event": "read-back", "payload": "plugin-hello:greeting"}),
    );
    assert!(denied.is_err(), "revoke 后 read 应拒，实际 {denied:?}");
}

/// DK-审计面：结构化审计链落环验证——真实执行（init/handle-event/零权限拒）
/// 产生 AuditEntry（eprintln 双写保留），audit_recent 可查询。
/// 产生面纪律：stderr 可观测 + ring 可查询（stderr 不再是唯一出口）。
#[test]
fn e2e_structured_audit_ring_records_real_executions() {
    let runtime = WasmRuntime::with_default_registry();

    // 1) 零权限件 init 拒——denied 审计行入环（零权限件先 register+install
    //    ——未注册件走 NotFound，不产生审计行，与既有 eprintln 语义一致）
    let Some(denied_bytes) = guest_wasm_bytes("plugin-hello-guest", "plugin_hello_guest.wasm")
    else {
        eprintln!("[e2e-skip] hello: wasm 产物缺失");
        return;
    };
    runtime.register("denied", CapabilityManifest::new());
    runtime.install_component("denied", denied_bytes);
    runtime
        .invoke("denied", "init", &serde_json::json!({}))
        .expect_err("zero-grant init must be denied");
    let denied_rows = runtime.audit_recent(50, Some("denied"));
    assert!(
        denied_rows
            .iter()
            .any(|e| e.event == "init" && e.result == "denied" && e.reason == "no-token"),
        "零权限拒应有结构化 denied 审计行: {:?}",
        denied_rows
    );

    // 2) 真件 init/handle-event ok——ok 审计行入环
    let Some(bytes) = guest_wasm_bytes("plugin-hello-guest", "plugin_hello_guest.wasm") else {
        eprintln!("[e2e-skip] hello: wasm 产物缺失");
        return;
    };
    runtime.install_component("plugin-hello", bytes);
    runtime.register(
        "plugin-hello",
        CapabilityManifest::from_permissions(&[
            "fs".into(),
            "log".into(),
            "storage".into(),
            "user-output".into(),
        ])
        .unwrap(),
    );
    runtime
        .invoke("plugin-hello", "init", &serde_json::json!({}))
        .expect("hello init ok");
    let ok_rows = runtime.audit_recent(50, Some("plugin-hello"));
    assert!(
        ok_rows
            .iter()
            .any(|e| e.event == "init" && e.result == "ok"),
        "hello init 应有 ok 审计行: {:?}",
        ok_rows
    );

    // 3) 查询面形态：ts 非空 + serde 序列化（tauri DTO 投影前置）
    let all = runtime.audit_recent(100, None);
    assert!(all.len() >= 2, "至少两条审计: {:?}", all.len());
    for e in &all {
        assert!(!e.ts.is_empty());
        assert!(serde_json::to_string(e).is_ok());
    }
    // 新→旧序（recent 语义）：最后一条比第一条不早（同 ts 秒级可能相等，比 plugin 序）
    assert_eq!(all.last().expect("non-empty").plugin, "plugin-hello");
}
