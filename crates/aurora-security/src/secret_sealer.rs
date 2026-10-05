//! DK-37 — `SecretSealer` 的 vault 实现：API key 等小体量 secret 的落库密封。
//!
//! 复用 [`LocalDekVault`] 的 DEK 体系（AES-256-GCM，内存 DEK Drop 擦除），
//! 输出为 vault.encrypt 的 bincode 密文，由 core 的 sealed 键规约 base64 落库。
//! 算法标记 `dek-v1` 与 core 的 `SEAL_ALG_DEK_V1` 对齐。

use crate::vault::LocalDekVault;
use aurora_core::l3_domain::system_settings::SecretSealer;
use std::sync::Arc;

/// vault 背书的密封器（bootstrap 装配注入）。
pub struct VaultSecretSealer {
    vault: Arc<LocalDekVault>,
    crypto: Arc<dyn aurora_core::traits::crypto_provider::CryptoProvider>,
}

impl VaultSecretSealer {
    pub fn new(
        vault: Arc<LocalDekVault>,
        crypto: Arc<dyn aurora_core::traits::crypto_provider::CryptoProvider>,
    ) -> Self {
        Self { vault, crypto }
    }
}

impl SecretSealer for VaultSecretSealer {
    fn seal(&self, plaintext: &[u8]) -> Result<Vec<u8>, String> {
        self.vault
            .encrypt(self.crypto.as_ref(), plaintext)
            .map_err(|e| e.to_string())
    }

    fn unseal(&self, sealed: &[u8]) -> Result<Vec<u8>, String> {
        self.vault
            .decrypt(self.crypto.as_ref(), sealed)
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto_provider_impl::SecurityCryptoProvider;
    use aurora_core::l3_domain::system_settings::{
        AISettings, SettingsLayer, SettingsStore, CLOUD_API_KEY_PLAIN_LEGACY, CLOUD_API_KEY_SEALED,
        CLOUD_API_KEY_SEAL_ALG, SEAL_ALG_DEK_V1,
    };

    fn sealer() -> VaultSecretSealer {
        VaultSecretSealer::new(
            Arc::new(
                LocalDekVault::load_or_create(&std::env::temp_dir().join("dk37-vault-test-keys"))
                    .unwrap(),
            ),
            Arc::new(SecurityCryptoProvider::new()),
        )
    }

    /// 往返：save_secret_to → load_secret_from 还原 key。
    #[test]
    fn test_sealed_roundtrip() {
        let store = SettingsStore::new();
        let s = sealer();
        let ai = AISettings {
            cloud_api_key: Some("sk-test-abc123".into()),
            ..AISettings::default()
        };
        ai.save_secret_to(&store, SettingsLayer::User, None, &s)
            .unwrap();

        // 落库形态核验：sealed 键存在+明文键已被清空。
        assert!(store.get_effective(None, CLOUD_API_KEY_SEALED).is_some());
        let legacy = store.get_effective(None, CLOUD_API_KEY_PLAIN_LEGACY);
        assert_eq!(legacy, Some(serde_json::Value::String(String::new())));

        let loaded = AISettings::load_secret_from(&store, None, &s);
        assert_eq!(loaded.cloud_api_key.as_deref(), Some("sk-test-abc123"));
    }

    /// 明文迁移：历史明文键 → save_secret_to 后 sealed 接管+明文清除。
    #[test]
    fn test_legacy_plaintext_migration() {
        let store = SettingsStore::new();
        let s = sealer();
        // 模拟历史数据：明文键在库。
        store.set(
            SettingsLayer::System,
            None,
            CLOUD_API_KEY_PLAIN_LEGACY,
            serde_json::Value::String("sk-legacy-key".into()),
        );
        // 迁移窗口：load_secret_from 走明文兼容路径可读出。
        let migrated_src = AISettings::load_secret_from(&store, None, &s);
        assert_eq!(migrated_src.cloud_api_key.as_deref(), Some("sk-legacy-key"));

        // 自迁移：save_secret_to → sealed 接管，明文清除。
        migrated_src
            .save_secret_to(&store, SettingsLayer::System, None, &s)
            .unwrap();
        let loaded = AISettings::load_secret_from(&store, None, &s);
        assert_eq!(loaded.cloud_api_key.as_deref(), Some("sk-legacy-key"));
        assert_eq!(
            store.get_effective(None, CLOUD_API_KEY_PLAIN_LEGACY),
            Some(serde_json::Value::String(String::new()))
        );
    }

    /// 损坏 sealed 密文 → warn 降级 None（不 panic、不误读）。
    #[test]
    fn test_corrupted_sealed_degrades_to_none() {
        let store = SettingsStore::new();
        let s = sealer();
        store.set(
            SettingsLayer::System,
            None,
            CLOUD_API_KEY_SEALED,
            serde_json::Value::String("bm90LXZhbGlkLWNpcGhlcnRleHQ".into()), // base64("not-valid-ciphertext")
        );
        store.set(
            SettingsLayer::System,
            None,
            CLOUD_API_KEY_SEAL_ALG,
            serde_json::Value::String(SEAL_ALG_DEK_V1.into()),
        );
        let loaded = AISettings::load_secret_from(&store, None, &s);
        assert_eq!(loaded.cloud_api_key, None);
        assert!(!loaded.cloud_configured());
    }
}
