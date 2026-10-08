//! DK-45 DoD 1/3：插件沙箱策略宿主入口——安装路径语义锚定 e2e。
//!
//! 桌面 tauri 层（plugin_commands.rs）为薄壳透传：本测在可链接 crate
//! （bootstrap——桌面装配层）用 core 原语直调，覆盖 command 背后的全部语义：
//! - DoD1: set → list roundtrip（publisher 隔离 + 缺省展示）
//! - DoD3: 安装路径锚定——set → apply → manifest.sandbox == limits；
//!   未配置 publisher → 保持缺省（DK-14 市场安装接入即用）

use aurora_core::l1_infrastructure::storage_engine::MemoryKVStore;

fn memory_kv() -> std::sync::Arc<MemoryKVStore> {
    std::sync::Arc::new(MemoryKVStore::default())
}
use aurora_core::traits::plugin_policy::{
    apply_sandbox_policy, plugin_sandbox_policy, set_plugin_sandbox_policy,
};
use aurora_core::traits::plugin_runtime::{PluginManifest, RuntimeType, SandboxLimits};

fn sample_manifest(author: &str, limits: SandboxLimits) -> PluginManifest {
    PluginManifest {
        id: "mkt-x".into(),
        name: "市场插件".into(),
        version: "1.0.0".into(),
        author: author.into(),
        description: String::new(),
        runtime: RuntimeType::Wasm,
        entry: "plugin.wasm".into(),
        permissions: vec![],
        hooks: vec![],
        block_types: vec![],
        config_schema: None,
        sandbox: limits,
    }
}

/// DoD 1: set → list roundtrip（publisher 隔离 + 缺省展示正确）。
#[tokio::test]
async fn dk45_policy_set_list_roundtrip() {
    let kv = memory_kv();
    // 配置 publisher-a（自定义限额）；publisher-b 未配置
    set_plugin_sandbox_policy(
        kv.as_ref(),
        "publisher-a",
        SandboxLimits {
            epoch_deadline_ticks: 50,
            max_memory_bytes: 8 * 1024 * 1024,
        },
    )
    .await
    .unwrap();

    let mut out = Vec::new();
    for p in ["publisher-a", "publisher-b"] {
        let policy = plugin_sandbox_policy(kv.as_ref(), p).await.unwrap();
        match policy {
            Some(l) => out.push((p.to_string(), l, true)),
            None => out.push((p.to_string(), SandboxLimits::default(), false)),
        }
    }

    assert_eq!(out.len(), 2, "publisher 隔离：两条独立展示");
    let (pa, la, ca) = &out[0];
    assert_eq!(pa, "publisher-a");
    assert!(*ca, "已配置须标记 true");
    assert_eq!(la.epoch_deadline_ticks, 50);
    assert_eq!(la.max_memory_bytes, 8 * 1024 * 1024);

    let (pb, lb, cb) = &out[1];
    assert_eq!(pb, "publisher-b");
    assert!(!*cb, "未配置须标记 false");
    let def = SandboxLimits::default();
    assert_eq!(
        lb.epoch_deadline_ticks, def.epoch_deadline_ticks,
        "缺省展示"
    );
    assert_eq!(lb.max_memory_bytes, def.max_memory_bytes);
}

/// DoD 3: 安装路径语义锚定——覆盖与保持双场景。
#[tokio::test]
async fn dk45_install_path_semantics_anchored() {
    let kv = memory_kv();

    // 场景 1：已配置 publisher → apply 覆盖（安装路径 to_manifest 之后、load 之前）
    let limits = SandboxLimits {
        epoch_deadline_ticks: 100,
        max_memory_bytes: 16 * 1024 * 1024,
    };
    set_plugin_sandbox_policy(kv.as_ref(), "pub-x", limits)
        .await
        .unwrap();
    let mut m = sample_manifest("pub-x", SandboxLimits::default());
    let policy = plugin_sandbox_policy(kv.as_ref(), "pub-x").await.unwrap();
    apply_sandbox_policy(&mut m, policy);
    assert_eq!(m.sandbox, limits, "安装路径须应用宿主策略覆盖");

    // 场景 2：未配置 publisher → manifest.sandbox 保持缺省（不动）
    let mut mb = sample_manifest("pub-unset", SandboxLimits::default());
    let policy_b = plugin_sandbox_policy(kv.as_ref(), "pub-unset")
        .await
        .unwrap();
    assert!(policy_b.is_none(), "未配置须 None");
    apply_sandbox_policy(&mut mb, policy_b);
    assert_eq!(
        mb.sandbox,
        SandboxLimits::default(),
        "未配置须保持 listing 缺省安全值"
    );
}
