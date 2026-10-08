//! 插件沙箱策略（DK-45 前置原语）——宿主差异化限额的持久化面。
//!
//! 背景：DK-43 的 `PluginListing.sandbox` 是 `#[serde(skip)]`（签名面外——
//! 限额是宿主策略非插件声明），市场 JSON 拉取后恒为缺省安全值。
//! 本模块提供「宿主策略」的落地原语：按 publisher 维度持久化限额映射，
//! 安装路径（to_manifest 之后、load 之前）应用覆盖——市场差异化限额闭环。
//!
//! 领地注记：core 面原语先行；desktop/tauri 接线（策略管理 UI + 安装 command
//! 调用点）排 DK-44 之后（Bravo desktop 领地，避免并行冲突）。
//!
//! 键空间：`settings:plugin_policy:{publisher}`（与 wifi_only 的
//! `settings:sync.wifi_only` 同域——settings 前缀即宿主策略面）。

use crate::error_codes::ErrorCode;
use crate::traits::kv_store::KVStore;
use crate::traits::plugin_runtime::{PluginManifest, SandboxLimits};
use crate::Error;

/// 策略键（publisher 维度——PluginManifest.author / listing 作者字段同源）。
fn policy_key(publisher: &str) -> String {
    format!("settings:plugin_policy:{publisher}")
}

/// 设置/更新某 publisher 的沙箱限额策略（宿主管理面写入）。
pub async fn set_plugin_sandbox_policy(
    kv: &dyn KVStore,
    publisher: &str,
    limits: SandboxLimits,
) -> Result<(), Error> {
    if publisher.trim().is_empty() {
        return Err(Error::Domain {
            code: ErrorCode::A04,
            message: "publisher 不能为空",
        });
    }
    let bytes = serde_json::to_vec(&limits)
        .map_err(|e| Error::Internal(format!("sandbox policy serialize: {e}")))?;
    kv.set(&policy_key(publisher), &bytes).await
}

/// 查询 publisher 策略；未配置返回 `None`（调用方回退 `SandboxLimits::default`）。
/// 存储损坏（坏 JSON）容错为 `None` + warn——策略缺失走安全缺省，不阻断安装。
pub async fn plugin_sandbox_policy(
    kv: &dyn KVStore,
    publisher: &str,
) -> Result<Option<SandboxLimits>, Error> {
    match kv.get(&policy_key(publisher)).await? {
        Some(bytes) if !bytes.is_empty() => match serde_json::from_slice(&bytes) {
            Ok(l) => Ok(Some(l)),
            Err(e) => {
                tracing::warn!(
                    "plugin policy corrupt（回退缺省安全值）: publisher={publisher}, err={e}"
                );
                Ok(None)
            }
        },
        _ => Ok(None),
    }
}

/// 应用策略到 manifest（安装路径：`to_manifest` 之后、`load` 之前）。
/// `None`（未配置）→ 不动 `manifest.sandbox`（保持 listing 缺省安全值——
/// 与 DK-42 的 0 值回退语义同构：invoke 侧再做一次 runtime 缺省兜底）。
pub fn apply_sandbox_policy(manifest: &mut PluginManifest, policy: Option<SandboxLimits>) {
    if let Some(limits) = policy {
        manifest.sandbox = limits;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::l1_infrastructure::storage_engine::MemoryKVStore;

    fn kv() -> std::sync::Arc<dyn KVStore> {
        std::sync::Arc::new(MemoryKVStore::default())
    }

    fn manifest(author: &str) -> PluginManifest {
        PluginManifest {
            id: "p1".into(),
            name: "P1".into(),
            version: "1.0.0".into(),
            author: author.into(),
            description: String::new(),
            runtime: crate::traits::plugin_runtime::RuntimeType::Wasm,
            entry: "p.wasm".into(),
            permissions: vec![],
            hooks: vec![],
            block_types: vec![],
            config_schema: None,
            sandbox: SandboxLimits::default(),
        }
    }

    /// DoD1: set/get roundtrip——策略持久化与读取一致。
    #[tokio::test]
    async fn policy_set_get_roundtrip() {
        let kv = kv();
        let limits = SandboxLimits {
            epoch_deadline_ticks: 50,
            max_memory_bytes: 8 * 1024 * 1024,
        };
        set_plugin_sandbox_policy(kv.as_ref(), "pub-a", limits)
            .await
            .unwrap();
        assert_eq!(
            plugin_sandbox_policy(kv.as_ref(), "pub-a").await.unwrap(),
            Some(limits)
        );
        // 未配置 publisher → None
        assert_eq!(
            plugin_sandbox_policy(kv.as_ref(), "pub-b").await.unwrap(),
            None
        );
    }

    /// DoD2: apply——已配置覆盖 / 未配置不动（缺省安全值保留）。
    #[tokio::test]
    async fn policy_apply_overrides_manifest() {
        let kv = kv();
        let limits = SandboxLimits {
            epoch_deadline_ticks: 30,
            max_memory_bytes: 4 * 1024 * 1024,
        };
        set_plugin_sandbox_policy(kv.as_ref(), "trusted", limits)
            .await
            .unwrap();

        // 已配置 publisher：manifest 被覆盖
        let mut m1 = manifest("trusted");
        apply_sandbox_policy(
            &mut m1,
            plugin_sandbox_policy(kv.as_ref(), "trusted").await.unwrap(),
        );
        assert_eq!(m1.sandbox, limits);

        // 未配置 publisher：manifest.sandbox 保持缺省安全值
        let mut m2 = manifest("unknown");
        apply_sandbox_policy(
            &mut m2,
            plugin_sandbox_policy(kv.as_ref(), "unknown").await.unwrap(),
        );
        assert_eq!(m2.sandbox, SandboxLimits::default());
    }

    /// DoD3: 多 publisher 隔离 + 空 publisher 拒绝 + 坏数据容错回退。
    #[tokio::test]
    async fn policy_isolation_and_corrupt_tolerance() {
        let kv = kv();
        set_plugin_sandbox_policy(
            kv.as_ref(),
            "pub-x",
            SandboxLimits {
                epoch_deadline_ticks: 10,
                max_memory_bytes: 1024,
            },
        )
        .await
        .unwrap();
        set_plugin_sandbox_policy(
            kv.as_ref(),
            "pub-y",
            SandboxLimits {
                epoch_deadline_ticks: 20,
                max_memory_bytes: 2048,
            },
        )
        .await
        .unwrap();
        // 隔离：互不串扰
        let x = plugin_sandbox_policy(kv.as_ref(), "pub-x").await.unwrap();
        let y = plugin_sandbox_policy(kv.as_ref(), "pub-y").await.unwrap();
        assert_ne!(x, y);
        assert_eq!(x.unwrap().epoch_deadline_ticks, 10);

        // 空 publisher 拒绝
        assert!(
            set_plugin_sandbox_policy(kv.as_ref(), "  ", SandboxLimits::default())
                .await
                .is_err()
        );

        // 坏 JSON 容错：None（安全缺省兜底）而非 Err（不阻断安装）
        kv.set(&policy_key("corrupt"), b"{not-json").await.unwrap();
        assert_eq!(
            plugin_sandbox_policy(kv.as_ref(), "corrupt").await.unwrap(),
            None
        );
    }
}
