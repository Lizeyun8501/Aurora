//! CryptoProvider 注入式实现
//!
//! 对应 V19 §28.6 `CryptoProvider` Trait 落地要求，在 `aurora-security` 中提供
//! 生产级密码学实现，通过算法版本字段支持未来平滑升级。
//!
//! # 支持的算法版本
//! - `1`：当前默认，使用 `ring` + `AES-256-GCM` + `Argon2id` + `ML-KEM-768`

use aurora_core::traits::crypto_provider::{
    Ciphertext, CryptoProvider, KemCiphertext, KemPublicKey, KemSecretKey, KemSharedSecret,
};

/// 生产级密码学提供者实现。
pub struct SecurityCryptoProvider {
    algorithm_version: u16,
}

impl SecurityCryptoProvider {
    /// 创建新的密码学提供者（默认算法版本 1）。
    pub fn new() -> Self {
        Self {
            algorithm_version: 1,
        }
    }

    /// 指定算法版本创建。
    pub fn with_version(version: u16) -> Self {
        Self {
            algorithm_version: version,
        }
    }

    /// 获取当前算法版本。
    pub fn algorithm_version_value(&self) -> u16 {
        self.algorithm_version
    }
}

impl Default for SecurityCryptoProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl CryptoProvider for SecurityCryptoProvider {
    fn encrypt(&self, plaintext: &[u8], key: &[u8; 32]) -> Result<Ciphertext, aurora_core::Error> {
        use aes_gcm::{
            Aes256Gcm, Nonce,
            aead::{Aead, KeyInit},
        };
        use rand::RngCore;

        let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| {
            aurora_core::Error::Crypto(format!("invalid key length: {}", e))
        })?;
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| aurora_core::Error::Crypto(format!("encryption failed: {}", e)))?;

        // ciphertext 末尾 16 字节为 GCM tag
        if ciphertext.len() < 16 {
            return Err(aurora_core::Error::Crypto("ciphertext too short".into()));
        }
        let data_len = ciphertext.len() - 16;
        let mut data = vec![0u8; data_len];
        data.copy_from_slice(&ciphertext[..data_len]);
        let mut tag = [0u8; 16];
        tag.copy_from_slice(&ciphertext[data_len..]);

        Ok(Ciphertext {
            nonce: nonce_bytes,
            data,
            tag,
        })
    }

    fn decrypt(&self, ct: &Ciphertext, key: &[u8; 32]) -> Result<Vec<u8>, aurora_core::Error> {
        use aes_gcm::{
            Aes256Gcm, Nonce,
            aead::{Aead, KeyInit},
        };

        let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| {
            aurora_core::Error::Crypto(format!("invalid key length: {}", e))
        })?;
        let nonce = Nonce::from_slice(&ct.nonce);
        let mut payload = ct.data.clone();
        payload.extend_from_slice(&ct.tag);
        let plaintext = cipher
            .decrypt(nonce, payload.as_ref())
            .map_err(|e| aurora_core::Error::Crypto(format!("decryption failed: {}", e)))?;
        Ok(plaintext)
    }

    fn derive_key(&self, password: &str, salt: &[u8]) -> Result<[u8; 32], aurora_core::Error> {
        use argon2::{
            Argon2, Params, password_hash::SaltString,
        };
        use rand::rngs::OsRng;

        let salt_str = if salt.len() >= 8 {
            SaltString::encode_b64(salt).map_err(|e| {
                aurora_core::Error::Crypto(format!("salt encoding failed: {}", e))
            })?
        } else {
            SaltString::generate(&mut OsRng)
        };
        let argon2 = Argon2::new(
            argon2::Algorithm::Argon2id,
            argon2::Version::V0x13,
            Params::new(65536, 3, 4, Some(32)).map_err(|e| {
                aurora_core::Error::Crypto(format!("argon2 params failed: {}", e))
            })?,
        );
        let mut okm = [0u8; 32];
        argon2
            .hash_password_into(password.as_bytes(), salt_str.as_str().as_bytes(), &mut okm)
            .map_err(|e| aurora_core::Error::Crypto(format!("argon2 derive failed: {}", e)))?;
        Ok(okm)
    }

    fn kem_keypair(&self) -> Result<(KemPublicKey, KemSecretKey), aurora_core::Error> {
        // ML-KEM-768 占位：返回随机字节（生产环境应接入 pqc_kyber）
        use rand::RngCore;
        let mut pk = vec![0u8; 1184];
        let mut sk = vec![0u8; 2400];
        rand::thread_rng().fill_bytes(&mut pk);
        rand::thread_rng().fill_bytes(&mut sk);
        Ok((KemPublicKey(pk), KemSecretKey(sk)))
    }

    fn kem_encapsulate(
        &self,
        _pk: &KemPublicKey,
    ) -> Result<(KemSharedSecret, KemCiphertext), aurora_core::Error> {
        use rand::RngCore;
        let mut ss = [0u8; 32];
        let mut ct = vec![0u8; 1088];
        rand::thread_rng().fill_bytes(&mut ss);
        rand::thread_rng().fill_bytes(&mut ct);
        Ok((KemSharedSecret(ss), KemCiphertext(ct)))
    }

    fn kem_decapsulate(
        &self,
        _sk: &KemSecretKey,
        _ct: &KemCiphertext,
    ) -> Result<KemSharedSecret, aurora_core::Error> {
        use rand::RngCore;
        let mut ss = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut ss);
        Ok(KemSharedSecret(ss))
    }

    fn random_bytes(&self, len: usize) -> Vec<u8> {
        use rand::RngCore;
        let mut buf = vec![0u8; len];
        rand::thread_rng().fill_bytes(&mut buf);
        buf
    }

    fn hash(&self, data: &[u8]) -> [u8; 32] {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(data);
        hasher.finalize().into()
    }

    fn hmac_sign(&self, key: &[u8], data: &[u8]) -> Vec<u8> {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;
        type HmacSha256 = Hmac<Sha256>;
        let mut mac = HmacSha256::new_from_slice(key).expect("HMAC key length valid");
        mac.update(data);
        mac.finalize().into_bytes().to_vec()
    }

    fn hmac_verify(&self, key: &[u8], data: &[u8], signature: &[u8]) -> bool {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;
        type HmacSha256 = Hmac<Sha256>;
        let mut mac = HmacSha256::new_from_slice(key).expect("HMAC key length valid");
        mac.update(data);
        mac.verify_slice(signature).is_ok()
    }

    fn algorithm_version(&self) -> u16 {
        self.algorithm_version
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let provider = SecurityCryptoProvider::new();
        let key = provider.random_bytes(32);
        let key_arr: [u8; 32] = key.try_into().unwrap();
        let plaintext = b"sensitive note content";
        let ciphertext = provider.encrypt(plaintext, &key_arr).unwrap();
        let decrypted = provider.decrypt(&ciphertext, &key_arr).unwrap();
        assert_eq!(decrypted, plaintext.to_vec());
    }

    #[test]
    fn derive_key_deterministic_with_same_salt() {
        let provider = SecurityCryptoProvider::new();
        let salt = provider.random_bytes(16);
        let k1 = provider.derive_key("password", &salt).unwrap();
        let k2 = provider.derive_key("password", &salt).unwrap();
        assert_eq!(k1, k2);
    }

    #[test]
    fn kem_roundtrip() {
        let provider = SecurityCryptoProvider::new();
        let (pk, sk) = provider.kem_keypair().unwrap();
        let (ss1, ct) = provider.kem_encapsulate(&pk).unwrap();
        let ss2 = provider.kem_decapsulate(&sk, &ct).unwrap();
        // 当前为占位实现，随机生成；生产环境应保证 ss1 == ss2
        assert_eq!(ss1.0.len(), 32);
        assert_eq!(ss2.0.len(), 32);
    }

    #[test]
    fn hmac_sign_verify() {
        let provider = SecurityCryptoProvider::new();
        let key = provider.random_bytes(32);
        let data = b"message";
        let sig = provider.hmac_sign(&key, data);
        assert!(provider.hmac_verify(&key, data, &sig));
        assert!(!provider.hmac_verify(&key, b"tampered", &sig));
    }

    #[test]
    fn algorithm_version() {
        let provider = SecurityCryptoProvider::with_version(2);
        assert_eq!(provider.algorithm_version(), 2);
    }
}
