//! 桌面安装面语义层（DK-市场卡 / 桌面安装面接线）。
//!
//! 职责：市场 listing → 安装语义链（纯函数，tauri 命令薄委托，可单测）。
//!
//! fail-closed 链（任一环拒绝即整链失败，不静默）：
//! 1. listing 存在性（market.get → NotFound）
//! 2. manifest.id 与 listing.id 一致性（防错装）
//! 3. wasm 字节 SHA-256 与 listing.checksum 对账（完整性；空 checksum 拒）
//! 4. `Plugin::new` 强签名（未知 capability/permission 串拒绝 + id 禁 `:`，
//!    DK-15a/16.1 fail-closed 借力）
//! 5. `WasmRuntime::register`（二道防线，同 `:` 堵口语义）
//! 6. KV 持久化（installed 元数据 + blob 字节——启动恢复数据源）
//!
//! 数据键约定：
//! - `plugins:installed:{id}` = serde_json(PluginManifest)
//! - `plugins:blob:{id}`      = wasm 字节原样
//!
//! 注：宿主预置键（`plugins:` 前缀）不进任何插件的 namespace 命名空间
//! （L1 强制前缀 `{plugin_id}:` 隔离不冲突）。

use crate::lifecycle::{Plugin, PluginMode};
use crate::marketplace::Marketplace;
use crate::wasm_runtime::WasmRuntime;
use aurora_core::traits::kv_store::KVStore;
use aurora_core::traits::plugin_runtime::PluginManifest;
use sha2::{Digest, Sha256};
use std::sync::Arc;

/// 已安装插件元数据键前缀（值 = serde_json(PluginManifest)）。
pub const INSTALLED_PREFIX: &str = "plugins:installed:";
/// 插件 wasm 字节键前缀（值 = wasm 原样字节，启动恢复数据源）。
pub const BLOB_PREFIX: &str = "plugins:blob:";

/// KVStore 错误域转接（aurora_core::Error → 插件系统统一错误）。
fn kv_err(e: aurora_core::Error) -> crate::Error {
    crate::Error::Kv(e.to_string())
}

/// 字节序列 hex 编码 SHA-256（与 listing.checksum 十六进制对账基准）。
pub fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// 安装语义链（见模块文档 fail-closed 链）。成功返回入库的 manifest 克隆。
///
/// # Errors
/// NotFound（listing 不存在）/ InvalidInput（id 不匹配、checksum 空、
/// checksum 不匹配、Plugin::new fail-closed）/ KV 写失败。
pub async fn install_plugin(
    runtime: &Arc<WasmRuntime>,
    kv: &Arc<dyn KVStore>,
    market: &Marketplace,
    listing_id: &str,
    manifest: PluginManifest,
    wasm_bytes: &[u8],
    reinstall: bool,
) -> Result<PluginManifest, crate::Error> {
    // 环 0：重装语义（fail-closed 默认拒——运行中插件不被静默替换；
    // 显式 reinstall=true 才允许覆盖，升级语义归 hot_update 金丝雀卡）
    if kv
        .get(&format!("{INSTALLED_PREFIX}{}", manifest.id))
        .await
        .map_err(kv_err)?
        .is_some()
        && !reinstall
    {
        return Err(crate::Error::InvalidInput(format!(
            "plugin `{}` already installed; pass reinstall=true to replace              (upgrade semantics belong to hot_update canary)",
            manifest.id
        )));
    }
    // 环 1：listing 存在性
    let listing = market
        .get(listing_id)
        .ok_or_else(|| crate::Error::NotFound(format!("listing not found: {listing_id}")))?;
    // 环 2：id 一致性（manifest 与 listing 错装拒绝）
    if manifest.id != listing.id {
        return Err(crate::Error::InvalidInput(format!(
            "manifest id `{}` does not match listing id `{listing_id}`",
            manifest.id
        )));
    }
    // 环 3：完整性对账（空 checksum 拒——fail-closed 不放行）
    if listing.checksum.is_empty() {
        return Err(crate::Error::InvalidInput(format!(
            "listing `{listing_id}` has empty checksum (fail-closed)"
        )));
    }
    let actual = sha256_hex(wasm_bytes);
    if actual != listing.checksum {
        return Err(crate::Error::InvalidInput(format!(
            "checksum mismatch for `{listing_id}`: expected {}, got {actual}",
            listing.checksum
        )));
    }
    // 环 4：Plugin::new 强签名（15a 未知串拒 / 16.1 `:` 拒）
    let plugin = Plugin::new(manifest.clone(), PluginMode::Wasm)
        .map_err(|e| crate::Error::InvalidManifest(e.to_string()))?;
    // 环 5：runtime 注册（二道防线）
    runtime.register(manifest.id.clone(), plugin.capabilities.clone());
    // 环 6：KV 持久化（元数据 + blob——启动恢复数据源）
    let meta = serde_json::to_vec(&manifest)
        .map_err(|e| crate::Error::InvalidManifest(format!("manifest serialize: {e}")))?;
    kv.set(&format!("{INSTALLED_PREFIX}{}", manifest.id), &meta)
        .await
        .map_err(kv_err)?;
    kv.set(&format!("{BLOB_PREFIX}{}", manifest.id), wasm_bytes)
        .await
        .map_err(kv_err)?;
    Ok(manifest)
}

/// 市场安装链（DK-市场下载器）：listing → **下载** → 六环复用。
///
/// 与 [`install_plugin`] 的唯一差异是字节来源：`wasm_bytes` 由
/// [`PluginFetcher`](crate::fetcher::PluginFetcher) 从 listing.download_url
/// 拉取（大小上限/超时 fail-closed 在 fetcher 层）。
///
/// **verified 语义表**（fail-closed）：
///
/// | source | listing.verified | require_verified | 结果 |
/// |---|---|---|---|
/// | Local | 任意 | 任意 | 放行（本地豁免） |
/// | Official/ThirdParty | true | 任意 | 放行 |
/// | Official/ThirdParty | false | true | **拒**（SignatureInvalid） |
/// | Official/ThirdParty | false | false | 放行（显式降权，调用方自担） |
///
/// # Errors
/// 同 [`install_plugin`] + Network（下载失败）/ InvalidInput（大小超限）/
/// SignatureInvalid（未 verified 拒装）。
/// 市场安装策略（参数对象化——clippy too_many_arguments 收口）。
/// 缺省=最保守：require_verified=true（fail-closed）+ reinstall=false（重装拒）。
#[derive(Debug, Clone, Copy)]
pub struct MarketInstallPolicy {
    pub require_verified: bool,
    pub reinstall: bool,
}

impl Default for MarketInstallPolicy {
    fn default() -> Self {
        Self {
            require_verified: true,
            reinstall: false,
        }
    }
}

pub async fn install_from_market(
    runtime: &Arc<WasmRuntime>,
    kv: &Arc<dyn KVStore>,
    market: &Marketplace,
    fetcher: &dyn crate::fetcher::PluginFetcher,
    listing_id: &str,
    manifest_override: Option<PluginManifest>,
    policy: MarketInstallPolicy,
) -> Result<PluginManifest, crate::Error> {
    let listing = market
        .get(listing_id)
        .ok_or_else(|| crate::Error::NotFound(format!("listing not found: {listing_id}")))?;
    // verified 环（市场专属，先于下载——拒装不花网络预算）
    let requires = policy.require_verified
        && !matches!(listing.source, crate::marketplace::PluginSource::Local);
    if requires && !listing.verified {
        return Err(crate::Error::SignatureInvalid(format!(
            "listing `{listing_id}` ({:?}) not verified; refuse to install (fail-closed)",
            listing.source
        )));
    }
    // manifest 派生（语义层——UI/调用方不拼 manifest）：override 优先
    // （未来随包 manifest 卡注入点），缺省走 DK-43 to_manifest 传播
    // （sandbox 差异化随 listing；permissions 零集=最小暴露面，权限授予链另卡）
    let manifest = manifest_override
        .unwrap_or_else(|| listing.to_manifest(String::new(), Vec::new(), Vec::new(), Vec::new()));
    let url = listing.download_url.clone();
    let wasm_bytes = fetcher.fetch(&url).await?;
    install_plugin(
        runtime,
        kv,
        market,
        listing_id,
        manifest,
        &wasm_bytes,
        policy.reinstall,
    )
    .await
}

/// 卸载：runtime 反注册（DK-17.1 shares 双向清理自动生效）+ KV 两键清除。
///
/// # Errors
/// NotFound（未安装）/ KV 删除失败。
pub async fn uninstall_plugin(
    runtime: &Arc<WasmRuntime>,
    kv: &Arc<dyn KVStore>,
    plugin_id: &str,
) -> Result<(), crate::Error> {
    let key = format!("{INSTALLED_PREFIX}{plugin_id}");
    if kv.get(&key).await.map_err(kv_err)?.is_none() {
        return Err(crate::Error::NotFound(format!(
            "plugin not installed: {plugin_id}"
        )));
    }
    runtime.unregister(plugin_id);
    kv.delete(&key).await.map_err(kv_err)?;
    kv.delete(&format!("{BLOB_PREFIX}{plugin_id}"))
        .await
        .map_err(kv_err)?;
    Ok(())
}

/// 已安装清单（KV 前缀扫描 → manifest 列表；损坏条目 fail-closed 上抛）。
///
/// # Errors
/// KV 读失败 / 条目反序列化失败（损坏即报，不静默跳过）。
pub async fn list_installed(kv: &Arc<dyn KVStore>) -> Result<Vec<PluginManifest>, crate::Error> {
    let rows = kv.scan_prefix(INSTALLED_PREFIX).await.map_err(kv_err)?;
    rows.into_iter()
        .map(|(key, bytes)| {
            serde_json::from_slice::<PluginManifest>(&bytes).map_err(|e| {
                crate::Error::InvalidInput(format!("installed meta corrupt ({key}): {e}"))
            })
        })
        .collect()
}

/// 启动恢复：扫描 KV 已安装件 → runtime 重新注册（blob 供重实例化）。
/// 返回恢复数；**恢复阶段损坏不阻断启动**——逐件尝试，失败留 warn 痕
/// （启动可用性优先于插件恢复；fail-closed 语义保留在安装/调用面）。
pub async fn restore_installed(runtime: &Arc<WasmRuntime>, kv: &Arc<dyn KVStore>) -> usize {
    let mut restored = 0usize;
    let Ok(rows) = kv.scan_prefix(INSTALLED_PREFIX).await else {
        eprintln!("[installer] restore scan failed (boot continues)");
        return 0;
    };
    for (key, bytes) in rows {
        let id = key
            .strip_prefix(INSTALLED_PREFIX)
            .unwrap_or(&key)
            .to_string();
        let manifest = match serde_json::from_slice::<PluginManifest>(&bytes) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("[installer] restore `{id}` meta corrupt, skipped: {e}");
                continue;
            }
        };
        let caps = match Plugin::new(manifest.clone(), PluginMode::Wasm) {
            Ok(p) => p.capabilities,
            Err(e) => {
                eprintln!("[installer] restore `{id}` manifest rejected, skipped: {e}");
                continue;
            }
        };
        runtime.register(id, caps);
        restored += 1;
    }
    restored
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::marketplace::{PluginListing, PluginSource};
    use aurora_core::traits::kv_store::KVStore;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// 内存 KV mock（测试用；async_trait 与生产 trait 同签名）。
    pub(crate) struct MemKv(pub(crate) Mutex<HashMap<String, Vec<u8>>>);

    #[async_trait::async_trait]
    impl KVStore for MemKv {
        async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, aurora_core::Error> {
            Ok(self.0.lock().unwrap().get(key).cloned())
        }
        async fn set(&self, key: &str, value: &[u8]) -> Result<(), aurora_core::Error> {
            self.0
                .lock()
                .unwrap()
                .insert(key.to_string(), value.to_vec());
            Ok(())
        }
        async fn delete(&self, key: &str) -> Result<(), aurora_core::Error> {
            self.0.lock().unwrap().remove(key);
            Ok(())
        }
        async fn exists(&self, key: &str) -> Result<bool, aurora_core::Error> {
            Ok(self.0.lock().unwrap().contains_key(key))
        }
        async fn batch_get(
            &self,
            keys: &[&str],
        ) -> Result<Vec<Option<Vec<u8>>>, aurora_core::Error> {
            let m = self.0.lock().unwrap();
            Ok(keys.iter().map(|k| m.get(*k).cloned()).collect())
        }
        async fn batch_set(&self, items: &[(&str, &[u8])]) -> Result<(), aurora_core::Error> {
            let mut m = self.0.lock().unwrap();
            for (k, v) in items {
                m.insert((*k).to_string(), v.to_vec());
            }
            Ok(())
        }
        async fn scan_prefix(
            &self,
            prefix: &str,
        ) -> Result<Vec<(String, Vec<u8>)>, aurora_core::Error> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .iter()
                .filter(|(k, _)| k.starts_with(prefix))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect())
        }
    }

    fn listing(id: &str, checksum: &str) -> PluginListing {
        PluginListing {
            id: id.into(),
            name: format!("插件 {id}"),
            version: "1.0.0".into(),
            author: "aurora".into(),
            description: "test".into(),
            source: PluginSource::Local,
            download_url: "local://test".into(),
            checksum: checksum.into(),
            verified: true,
            updated_at: chrono::Utc::now(),
            sandbox: Default::default(),
        }
    }

    fn manifest(id: &str) -> PluginManifest {
        serde_json::from_value(serde_json::json!({
            "id": id, "name": "测试件", "version": "1.0.0", "author": "aurora",
            "description": "安装链测试件", "runtime": "Wasm", "entry": "plugin.wasm",
            "permissions": ["storage"], "hooks": [], "block_types": []
        }))
        .expect("test manifest valid")
    }

    fn market_with(id: &str, checksum: &str) -> Marketplace {
        let m = Marketplace::new(Arc::new(
            aurora_security::crypto_provider_impl::SecurityCryptoProvider::new(),
        ));
        m.publish(listing(id, checksum));
        m
    }

    const WASM: &[u8] = b"fake-wasm-bytes";

    #[tokio::test]
    async fn test_install_full_chain_ok() {
        let runtime = Arc::new(WasmRuntime::with_default_registry());
        let kv: Arc<dyn KVStore> = Arc::new(MemKv(Mutex::new(HashMap::new())));
        let market = market_with("hello", &sha256_hex(WASM));
        let m = install_plugin(
            &runtime,
            &kv,
            &market,
            "hello",
            manifest("hello"),
            WASM,
            false,
        )
        .await
        .expect("install ok");
        assert_eq!(m.id, "hello");
        // runtime 已注册 + KV 两键落
        assert!(runtime.manifest("hello").is_some());
        assert!(kv.get("plugins:installed:hello").await.unwrap().is_some());
        assert_eq!(
            kv.get("plugins:blob:hello").await.unwrap().as_deref(),
            Some(WASM)
        );
    }

    // ---------- DK-市场下载器：install_from_market 全链（MockFetcher） ----------

    use crate::fetcher::MockFetcher;

    fn market_listing(
        id: &str,
        checksum: &str,
        source: PluginSource,
        verified: bool,
    ) -> Marketplace {
        let m = Marketplace::new(Arc::new(
            aurora_security::crypto_provider_impl::SecurityCryptoProvider::new(),
        ));
        let mut l = listing(id, checksum);
        l.source = source;
        l.verified = verified;
        m.publish(l);
        m
    }

    #[tokio::test]
    async fn test_market_install_success_chain() {
        let runtime = Arc::new(WasmRuntime::with_default_registry());
        let kv: Arc<dyn KVStore> = Arc::new(MemKv(Mutex::new(HashMap::new())));
        let market = market_listing("hello", &sha256_hex(WASM), PluginSource::Local, false);
        let fetcher = MockFetcher::new();
        fetcher.route("local://test", WASM.to_vec());
        let m = install_from_market(
            &runtime,
            &kv,
            &market,
            &fetcher,
            "hello",
            None,
            MarketInstallPolicy {
                require_verified: true,
                reinstall: false,
            },
        )
        .await
        .expect("Local 豁免 verified，全链应过");
        assert_eq!(m.id, "hello");
        assert_eq!(
            kv.get("plugins:blob:hello").await.unwrap().as_deref(),
            Some(WASM)
        );
    }

    #[tokio::test]
    async fn test_market_install_unverified_official_rejected() {
        let runtime = Arc::new(WasmRuntime::with_default_registry());
        let kv: Arc<dyn KVStore> = Arc::new(MemKv(Mutex::new(HashMap::new())));
        let market = market_listing("hello", &sha256_hex(WASM), PluginSource::Official, false);
        let fetcher = MockFetcher::new();
        fetcher.route("local://test", WASM.to_vec());
        let err = install_from_market(
            &runtime,
            &kv,
            &market,
            &fetcher,
            "hello",
            None,
            MarketInstallPolicy {
                require_verified: true,
                reinstall: false,
            },
        )
        .await
        .unwrap_err();
        assert!(
            matches!(err, crate::Error::SignatureInvalid(ref m) if m.contains("not verified")),
            "Official 未 verified 应拒装，实际 {err:?}"
        );
    }

    #[tokio::test]
    async fn test_market_install_require_verified_false_downgrades() {
        // 显式降权通道：require_verified=false → 未 verified 放行（调用方自担）
        let runtime = Arc::new(WasmRuntime::with_default_registry());
        let kv: Arc<dyn KVStore> = Arc::new(MemKv(Mutex::new(HashMap::new())));
        let market = market_listing("hello", &sha256_hex(WASM), PluginSource::ThirdParty, false);
        let fetcher = MockFetcher::new();
        fetcher.route("local://test", WASM.to_vec());
        install_from_market(
            &runtime,
            &kv,
            &market,
            &fetcher,
            "hello",
            None,
            MarketInstallPolicy {
                require_verified: false,
                reinstall: false,
            },
        )
        .await
        .expect("require_verified=false 显式降权应放行");
    }

    #[tokio::test]
    async fn test_market_install_verified_official_ok() {
        let runtime = Arc::new(WasmRuntime::with_default_registry());
        let kv: Arc<dyn KVStore> = Arc::new(MemKv(Mutex::new(HashMap::new())));
        let market = market_listing("hello", &sha256_hex(WASM), PluginSource::Official, true);
        let fetcher = MockFetcher::new();
        fetcher.route("local://test", WASM.to_vec());
        install_from_market(
            &runtime,
            &kv,
            &market,
            &fetcher,
            "hello",
            None,
            MarketInstallPolicy {
                require_verified: true,
                reinstall: false,
            },
        )
        .await
        .expect("Official verified=true 应过");
    }

    #[tokio::test]
    async fn test_market_install_download_fail_propagates() {
        let runtime = Arc::new(WasmRuntime::with_default_registry());
        let kv: Arc<dyn KVStore> = Arc::new(MemKv(Mutex::new(HashMap::new())));
        let market = market_listing("hello", &sha256_hex(WASM), PluginSource::Local, true);
        // Miss：fetcher 无此 url → Network 错误传播（六环不触达）
        let fetcher = MockFetcher::new();
        let err = install_from_market(
            &runtime,
            &kv,
            &market,
            &fetcher,
            "hello",
            None,
            MarketInstallPolicy {
                require_verified: true,
                reinstall: false,
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, crate::Error::Network(_)));
        assert!(kv.get("plugins:installed:hello").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_market_install_oversize_rejected_before_checksum() {
        // 超限字节：fetcher 层 fail-closed（InvalidInput），六环不触达
        let runtime = Arc::new(WasmRuntime::with_default_registry());
        let kv: Arc<dyn KVStore> = Arc::new(MemKv(Mutex::new(HashMap::new())));
        let huge = vec![0u8; crate::fetcher::MAX_PLUGIN_BYTES + 1];
        let market = market_listing("hello", &sha256_hex(&huge), PluginSource::Local, true);
        let fetcher = MockFetcher::new();
        fetcher.route("local://test", huge);
        let err = install_from_market(
            &runtime,
            &kv,
            &market,
            &fetcher,
            "hello",
            None,
            MarketInstallPolicy {
                require_verified: true,
                reinstall: false,
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, crate::Error::InvalidInput(_)));
    }

    #[tokio::test]
    async fn test_install_checksum_mismatch_rejected() {
        let runtime = Arc::new(WasmRuntime::with_default_registry());
        let kv: Arc<dyn KVStore> = Arc::new(MemKv(Mutex::new(HashMap::new())));
        let market = market_with("hello", "deadbeef");
        let err = install_plugin(
            &runtime,
            &kv,
            &market,
            "hello",
            manifest("hello"),
            WASM,
            false,
        )
        .await
        .unwrap_err();
        assert!(
            matches!(err, crate::Error::InvalidInput(ref m) if m.contains("checksum mismatch"))
        );
    }

    #[tokio::test]
    async fn test_install_id_mismatch_rejected() {
        let runtime = Arc::new(WasmRuntime::with_default_registry());
        let kv: Arc<dyn KVStore> = Arc::new(MemKv(Mutex::new(HashMap::new())));
        let market = market_with("hello", &sha256_hex(WASM));
        let err = install_plugin(
            &runtime,
            &kv,
            &market,
            "hello",
            manifest("other"),
            WASM,
            false,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, crate::Error::InvalidInput(ref m) if m.contains("does not match")));
    }

    #[tokio::test]
    async fn test_install_listing_missing_rejected() {
        let runtime = Arc::new(WasmRuntime::with_default_registry());
        let kv: Arc<dyn KVStore> = Arc::new(MemKv(Mutex::new(HashMap::new())));
        let market = Marketplace::new(Arc::new(
            aurora_security::crypto_provider_impl::SecurityCryptoProvider::new(),
        ));
        let err = install_plugin(
            &runtime,
            &kv,
            &market,
            "ghost",
            manifest("ghost"),
            WASM,
            false,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, crate::Error::NotFound(_)));
    }

    #[tokio::test]
    async fn test_install_unknown_permission_fail_closed() {
        let runtime = Arc::new(WasmRuntime::with_default_registry());
        let kv: Arc<dyn KVStore> = Arc::new(MemKv(Mutex::new(HashMap::new())));
        let market = market_with("hello", &sha256_hex(WASM));
        let mut bad = manifest("hello");
        bad.permissions = vec!["storage".into(), "bogus-perm".into()];
        let err = install_plugin(&runtime, &kv, &market, "hello", bad, WASM, false)
            .await
            .unwrap_err();
        // Plugin::new 强签名拒绝（15a fail-closed 借力，installer 转
        // InvalidManifest），KV 不落
        assert!(matches!(err, crate::Error::InvalidManifest(ref m) if m.contains("bogus-perm")));
        assert!(kv.scan_prefix("plugins:").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_uninstall_and_list_roundtrip() {
        let rt = Arc::new(WasmRuntime::with_default_registry());
        let kv: Arc<dyn KVStore> = Arc::new(MemKv(Mutex::new(HashMap::new())));
        let market = market_with("hello", &sha256_hex(WASM));
        install_plugin(&rt, &kv, &market, "hello", manifest("hello"), WASM, false)
            .await
            .unwrap();
        assert_eq!(list_installed(&kv).await.unwrap().len(), 1);
        uninstall_plugin(&rt, &kv, "hello").await.unwrap();
        assert!(list_installed(&kv).await.unwrap().is_empty());
        assert!(rt.manifest("hello").is_none());
        // 重复卸载 NotFound
        let err = uninstall_plugin(&rt, &kv, "hello").await.unwrap_err();
        assert!(matches!(err, crate::Error::NotFound(_)));
    }

    #[tokio::test]
    async fn test_reinstall_semantics() {
        let rt = Arc::new(WasmRuntime::with_default_registry());
        let kv: Arc<dyn KVStore> = Arc::new(MemKv(Mutex::new(HashMap::new())));
        let market = market_with("hello", &sha256_hex(WASM));

        // 首装成功
        install_plugin(&rt, &kv, &market, "hello", manifest("hello"), WASM, false)
            .await
            .expect("first install ok");

        // 重复安装默认拒（运行中插件不被静默替换）
        let err = install_plugin(&rt, &kv, &market, "hello", manifest("hello"), WASM, false)
            .await
            .unwrap_err();
        assert!(
            matches!(err, crate::Error::InvalidInput(ref m) if m.contains("already installed")),
            "重装拒语义: {err:?}"
        );

        // 显式 reinstall=true 放行（覆盖=升级语义入口，金丝雀归 hot_update）
        install_plugin(&rt, &kv, &market, "hello", manifest("hello"), WASM, true)
            .await
            .expect("reinstall=true 放行");
        assert!(rt.manifest("hello").is_some());
        assert!(kv.get("plugins:installed:hello").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn test_restore_installed_re_registers() {
        let rt = Arc::new(WasmRuntime::with_default_registry());
        let kv: Arc<dyn KVStore> = Arc::new(MemKv(Mutex::new(HashMap::new())));
        let market = market_with("hello", &sha256_hex(WASM));
        install_plugin(&rt, &kv, &market, "hello", manifest("hello"), WASM, false)
            .await
            .unwrap();
        // 模拟重启：新 runtime 实例
        let rt2 = Arc::new(WasmRuntime::with_default_registry());
        assert_eq!(restore_installed(&rt2, &kv).await, 1);
        assert!(rt2.manifest("hello").is_some());
    }
}
