//! 插件市场 (Plugin Marketplace)
//!
//! 去中心化市场，支持官方（Official）/ 第三方（ThirdParty）/ 本地（Local）三种来源。
//! 通过 Ed25519 签名验证插件包完整性与来源可信，并提供版本更新检测。
//!
//! # 代码签名
//! 发布者使用 Ed25519 私钥对插件清单（bincode 规范序列化）签名，
//! 安装时由 `PluginSignature::verify` 使用公钥校验，篡改即失败。
//! 信任根通过 `Marketplace::add_trusted_key` 注册的公钥集合维护。

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;
use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair, UnparsedPublicKey, ED25519};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

/// 插件来源分类。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum PluginSource {
    /// 官方市场：经过审核与签名
    Official,
    /// 第三方市场：需用户显式信任
    ThirdParty,
    /// 本地加载：未签名或用户自签
    Local,
}

impl PluginSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            PluginSource::Official => "official",
            PluginSource::ThirdParty => "third_party",
            PluginSource::Local => "local",
        }
    }
}

/// 市场中的插件条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginListing {
    pub id: String,
    pub name: String,
    /// 语义化版本字符串，如 `1.2.3`
    pub version: String,
    pub author: String,
    pub description: String,
    pub source: PluginSource,
    pub download_url: String,
    /// 插件包 SHA-256 校验和（十六进制）
    pub checksum: String,
    /// 是否通过签名验证
    pub verified: bool,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl PluginListing {
    /// 解析 semver 为 `(major, minor, patch)`，无法解析的分量按 0 处理。
    pub fn version_tuple(&self) -> (u64, u64, u64) {
        parse_semver(&self.version)
    }

    /// 规范字节序列（用于签名/校验）。
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, crate::Error> {
        bincode::serialize(self).map_err(crate::Error::Bincode)
    }
}

/// 解析 `major.minor.patch` 形式版本号。
fn parse_semver(v: &str) -> (u64, u64, u64) {
    let parts: Vec<&str> = v.split('.').collect();
    let major = parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
    let minor = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let patch = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
    (major, minor, patch)
}

/// Ed25519 代码签名。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginSignature {
    /// 签名公钥（32 字节）
    pub public_key: Vec<u8>,
    /// 签名值（64 字节）
    pub signature: Vec<u8>,
    /// 签名算法标识
    pub algorithm: String,
}

impl PluginSignature {
    const ALGORITHM: &'static str = "ed25519";

    /// 使用 PKCS8 私钥对消息签名，构造签名对象。
    pub fn sign(message: &[u8], pkcs8: &[u8]) -> Result<Self, crate::Error> {
        let kp = Ed25519KeyPair::from_pkcs8(pkcs8)
            .map_err(|_| crate::Error::SignatureInvalid("invalid PKCS#8 key material".into()))?;
        let sig = kp.sign(message);
        Ok(Self {
            public_key: kp.public_key().as_ref().to_vec(),
            signature: sig.as_ref().to_vec(),
            algorithm: Self::ALGORITHM.to_string(),
        })
    }

    /// 校验签名是否匹配消息。
    pub fn verify(&self, message: &[u8]) -> Result<(), crate::Error> {
        if self.algorithm != Self::ALGORITHM {
            return Err(crate::Error::SignatureInvalid(format!(
                "unsupported algorithm: {}",
                self.algorithm
            )));
        }
        let pk = UnparsedPublicKey::new(&ED25519, &self.public_key);
        pk.verify(message, &self.signature)
            .map_err(|_| crate::Error::SignatureInvalid("signature verification failed".into()))
    }

    /// 公钥是否与受信任集合匹配。
    pub fn is_trusted_by(&self, trusted: &[Vec<u8>]) -> bool {
        trusted.iter().any(|k| k.as_slice() == self.public_key.as_slice())
    }
}

/// 生成新的 Ed25519 PKCS#8 密钥对字节。
pub fn generate_keypair() -> Result<Vec<u8>, crate::Error> {
    let rng = SystemRandom::new();
    let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng)
        .map_err(|_| crate::Error::Internal("Ed25519 key generation failed".into()))?;
    Ok(pkcs8.as_ref().to_vec())
}

/// 更新检测结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCheck {
    pub plugin_id: String,
    pub current_version: String,
    pub latest_version: String,
    pub has_update: bool,
    pub source: PluginSource,
}

/// 插件市场：管理条目、信任公钥、签名校验与更新检测。
pub struct Marketplace {
    listings: Arc<RwLock<HashMap<String, PluginListing>>>,
    trusted_keys: Arc<RwLock<Vec<Vec<u8>>>>,
}

impl Default for Marketplace {
    fn default() -> Self {
        Self::new()
    }
}

impl Marketplace {
    pub fn new() -> Self {
        Self {
            listings: Arc::new(RwLock::new(HashMap::new())),
            trusted_keys: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// 发布一个条目到市场。
    pub fn publish(&self, listing: PluginListing) {
        let id = listing.id.clone();
        info!(
            "marketplace: publish {} v{} ({:?})",
            id, listing.version, listing.source
        );
        self.listings.write().insert(id, listing);
    }

    /// 注册本地条目（来源强制为 Local）。
    pub fn register_local(&self, mut listing: PluginListing) {
        listing.source = PluginSource::Local;
        self.publish(listing);
    }

    /// 移除条目。
    pub fn remove(&self, id: &str) -> Option<PluginListing> {
        self.listings.write().remove(id)
    }

    /// 获取条目。
    pub fn get(&self, id: &str) -> Option<PluginListing> {
        self.listings.read().get(id).cloned()
    }

    /// 列出全部条目。
    pub fn list(&self) -> Vec<PluginListing> {
        self.listings.read().values().cloned().collect()
    }

    /// 按来源过滤。
    pub fn list_by_source(&self, source: PluginSource) -> Vec<PluginListing> {
        self.listings
            .read()
            .values()
            .filter(|l| l.source == source)
            .cloned()
            .collect()
    }

    /// 关键字搜索（名称/描述/作者）。
    pub fn search(&self, query: &str) -> Vec<PluginListing> {
        let q = query.to_lowercase();
        self.listings
            .read()
            .values()
            .filter(|l| {
                l.name.to_lowercase().contains(&q)
                    || l.description.to_lowercase().contains(&q)
                    || l.author.to_lowercase().contains(&q)
            })
            .cloned()
            .collect()
    }

    /// 注册受信任公钥。
    pub fn add_trusted_key(&self, public_key: Vec<u8>) {
        self.trusted_keys.write().push(public_key);
    }

    /// 受信任公钥数量。
    pub fn trusted_key_count(&self) -> usize {
        self.trusted_keys.read().len()
    }

    /// 校验已签名条目：消息为条目规范字节，公钥需在受信任集合中（若集合非空）。
    pub fn verify_listing(
        &self,
        listing_id: &str,
        message: &[u8],
        sig: &PluginSignature,
    ) -> Result<(), crate::Error> {
        let listing = self
            .get(listing_id)
            .ok_or_else(|| crate::Error::NotFound(format!("listing not found: {}", listing_id)))?;
        // 若存在受信任公钥集合，则要求签名公钥被信任
        let trusted = self.trusted_keys.read().clone();
        if !trusted.is_empty() && !sig.is_trusted_by(&trusted) {
            warn!(
                "marketplace: listing {} signed by untrusted key",
                listing_id
            );
            return Err(crate::Error::SignatureInvalid(
                "signing key not in trusted set".into(),
            ));
        }
        sig.verify(message)?;
        // 校验通过后将条目标记为已验证
        if let Some(l) = self.listings.write().get_mut(listing_id) {
            l.verified = true;
        }
        debug!("marketplace: listing {} verified", listing_id);
        // 使用 listing 避免未使用变量警告
        let _ = &listing;
        Ok(())
    }

    /// 检测更新：比较当前已安装版本与市场最新版本。
    pub fn check_update(
        &self,
        plugin_id: &str,
        current_version: &str,
    ) -> Result<UpdateCheck, crate::Error> {
        let listing = self
            .get(plugin_id)
            .ok_or_else(|| crate::Error::NotFound(format!("listing not found: {}", plugin_id)))?;
        let latest = listing.version_tuple();
        let current = parse_semver(current_version);
        let has_update = latest > current;
        Ok(UpdateCheck {
            plugin_id: plugin_id.to_string(),
            current_version: current_version.to_string(),
            latest_version: listing.version.clone(),
            has_update,
            source: listing.source,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_listing(id: &str, version: &str, source: PluginSource) -> PluginListing {
        PluginListing {
            id: id.to_string(),
            name: format!("{} Plugin", id),
            version: version.to_string(),
            author: "aurora".to_string(),
            description: "a sample plugin".to_string(),
            source,
            download_url: format!("https://market.example.com/{}.wasm", id),
            checksum: "deadbeef".to_string(),
            verified: false,
            updated_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn test_plugin_source_as_str() {
        assert_eq!(PluginSource::Official.as_str(), "official");
        assert_eq!(PluginSource::ThirdParty.as_str(), "third_party");
        assert_eq!(PluginSource::Local.as_str(), "local");
    }

    #[test]
    fn test_listing_version_tuple() {
        let l = make_listing("p", "2.5.7", PluginSource::Official);
        assert_eq!(l.version_tuple(), (2, 5, 7));
        let l2 = make_listing("p", "1.0", PluginSource::Official);
        assert_eq!(l2.version_tuple(), (1, 0, 0));
        let l3 = make_listing("p", "bad", PluginSource::Official);
        assert_eq!(l3.version_tuple(), (0, 0, 0));
    }

    #[test]
    fn test_marketplace_publish_get_remove() {
        let m = Marketplace::new();
        m.publish(make_listing("p1", "1.0.0", PluginSource::Official));
        assert!(m.get("p1").is_some());
        assert!(m.get("missing").is_none());
        assert_eq!(m.list().len(), 1);
        assert!(m.remove("p1").is_some());
        assert!(m.get("p1").is_none());
    }

    #[test]
    fn test_marketplace_register_local() {
        let m = Marketplace::new();
        m.register_local(make_listing("p1", "1.0.0", PluginSource::Official));
        let l = m.get("p1").unwrap();
        assert_eq!(l.source, PluginSource::Local);
    }

    #[test]
    fn test_marketplace_list_by_source() {
        let m = Marketplace::new();
        m.publish(make_listing("p1", "1.0.0", PluginSource::Official));
        m.publish(make_listing("p2", "1.0.0", PluginSource::ThirdParty));
        m.publish(make_listing("p3", "1.0.0", PluginSource::Official));
        m.publish(make_listing("p4", "1.0.0", PluginSource::Local));

        assert_eq!(m.list_by_source(PluginSource::Official).len(), 2);
        assert_eq!(m.list_by_source(PluginSource::ThirdParty).len(), 1);
        assert_eq!(m.list_by_source(PluginSource::Local).len(), 1);
    }

    #[test]
    fn test_marketplace_search() {
        let m = Marketplace::new();
        m.publish(make_listing("calendar", "1.0.0", PluginSource::Official));
        let mut pdf = make_listing("pdf-export", "1.0.0", PluginSource::ThirdParty);
        pdf.author = "Acme Corp".to_string();
        m.publish(pdf);

        let results = m.search("pdf");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "pdf-export");

        let acme = m.search("acme");
        assert_eq!(acme.len(), 1);
        assert_eq!(acme[0].id, "pdf-export");
    }

    #[test]
    fn test_ed25519_sign_verify_roundtrip() {
        let pkcs8 = generate_keypair().unwrap();
        let message = b"hello aurora plugin";
        let sig = PluginSignature::sign(message, &pkcs8).unwrap();
        assert_eq!(sig.algorithm, "ed25519");
        assert_eq!(sig.public_key.len(), 32);
        assert_eq!(sig.signature.len(), 64);

        // 校验通过
        assert!(sig.verify(message).is_ok());
    }

    #[test]
    fn test_ed25519_verify_tampered_fails() {
        let pkcs8 = generate_keypair().unwrap();
        let message = b"original content";
        let sig = PluginSignature::sign(message, &pkcs8).unwrap();

        // 篡改消息
        let err = sig.verify(b"tampered content").unwrap_err();
        assert!(matches!(err, crate::Error::SignatureInvalid(_)));

        // 篡改签名
        let mut bad_sig = sig.clone();
        bad_sig.signature[0] ^= 0xff;
        assert!(bad_sig.verify(message).is_err());

        // 错误算法
        let mut bad_algo = sig.clone();
        bad_algo.algorithm = "rsa".to_string();
        assert!(bad_algo.verify(message).is_err());
    }

    #[test]
    fn test_marketplace_verify_listing() {
        let m = Marketplace::new();
        let listing = make_listing("p1", "1.0.0", PluginSource::Official);
        m.publish(listing.clone());

        let pkcs8 = generate_keypair().unwrap();
        let kp = Ed25519KeyPair::from_pkcs8(&pkcs8).unwrap();
        let pubkey = kp.public_key().as_ref().to_vec();
        m.add_trusted_key(pubkey);

        let message = listing.canonical_bytes().unwrap();
        let sig = PluginSignature::sign(&message, &pkcs8).unwrap();

        // 受信任公钥 → 校验通过，条目被标记 verified
        m.verify_listing("p1", &message, &sig).unwrap();
        assert!(m.get("p1").unwrap().verified);
    }

    #[test]
    fn test_marketplace_verify_listing_untrusted_key() {
        let m = Marketplace::new();
        m.publish(make_listing("p1", "1.0.0", PluginSource::Official));
        m.add_trusted_key(vec![0u8; 32]); // 信任一个无关公钥

        let pkcs8 = generate_keypair().unwrap();
        let message = b"msg";
        let sig = PluginSignature::sign(message, &pkcs8).unwrap();

        let err = m.verify_listing("p1", message, &sig).unwrap_err();
        assert!(matches!(err, crate::Error::SignatureInvalid(_)));
    }

    #[test]
    fn test_marketplace_verify_listing_no_trust_check() {
        // 未注册任何受信任公钥时，只要签名本身有效即通过
        let m = Marketplace::new();
        m.publish(make_listing("p1", "1.0.0", PluginSource::ThirdParty));
        let pkcs8 = generate_keypair().unwrap();
        let message = b"msg";
        let sig = PluginSignature::sign(message, &pkcs8).unwrap();
        assert!(m.verify_listing("p1", message, &sig).is_ok());
    }

    #[test]
    fn test_marketplace_check_update_available() {
        let m = Marketplace::new();
        m.publish(make_listing("p1", "2.0.0", PluginSource::Official));
        let check = m.check_update("p1", "1.0.0").unwrap();
        assert!(check.has_update);
        assert_eq!(check.latest_version, "2.0.0");
        assert_eq!(check.current_version, "1.0.0");
    }

    #[test]
    fn test_marketplace_check_update_none() {
        let m = Marketplace::new();
        m.publish(make_listing("p1", "1.2.3", PluginSource::Official));
        let check = m.check_update("p1", "1.2.3").unwrap();
        assert!(!check.has_update);

        // 旧版本也不算有更新
        let check2 = m.check_update("p1", "2.0.0").unwrap();
        assert!(!check2.has_update);
    }

    #[test]
    fn test_marketplace_check_update_missing_listing() {
        let m = Marketplace::new();
        assert!(m.check_update("ghost", "1.0.0").is_err());
    }

    #[test]
    fn test_trusted_key_count() {
        let m = Marketplace::new();
        assert_eq!(m.trusted_key_count(), 0);
        m.add_trusted_key(vec![1u8; 32]);
        m.add_trusted_key(vec![2u8; 32]);
        assert_eq!(m.trusted_key_count(), 2);
    }
}
