//! 端到端加密 (End-to-End Encryption)
//!
//! 基于 AES-256-GCM 的对称加密：明文经 DEK 加密为密文（含 nonce + GCM tag），
//! 采用零知识架构——服务端只存储/同步密文，无法解密。
//!
//! # 加密结构
//! - `Ciphertext = nonce(12B) || ciphertext_with_tag`
//! - AES-256-GCM 提供 96-bit nonce 与 128-bit 认证标签，保证机密性与完整性。
//! - 每次加密生成全新随机 nonce，杜绝 nonce 重用风险。

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use crate::{Error, WorkspaceDek};

/// AES-256-GCM nonce 长度（12 字节）
pub const NONCE_LEN: usize = 12;
/// GCM 认证标签长度（16 字节）
pub const TAG_LEN: usize = 16;

/// 密文：nonce + 加密数据（含 GCM tag）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Ciphertext {
    /// 12 字节 GCM nonce
    pub nonce: Vec<u8>,
    /// 加密后的密文（含末尾 16 字节 tag）
    pub ciphertext: Vec<u8>,
}

impl Ciphertext {
    pub fn new(nonce: Vec<u8>, ciphertext: Vec<u8>) -> Self {
        Self { nonce, ciphertext }
    }

    /// 序列化为紧凑字节数组（nonce || ciphertext），用于存储/同步
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(NONCE_LEN + self.ciphertext.len());
        out.extend_from_slice(&self.nonce);
        out.extend_from_slice(&self.ciphertext);
        out
    }

    /// 从字节数组解析（逆操作 of [`Ciphertext::to_bytes`]）
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() < NONCE_LEN {
            return Err(Error::InvalidInput(format!(
                "ciphertext too short: {} bytes (< {})",
                bytes.len(),
                NONCE_LEN
            )));
        }
        let (nonce, ciphertext) = bytes.split_at(NONCE_LEN);
        Ok(Self {
            nonce: nonce.to_vec(),
            ciphertext: ciphertext.to_vec(),
        })
    }

    /// 密文总长度（nonce + ciphertext + tag）
    pub fn total_len(&self) -> usize {
        self.nonce.len() + self.ciphertext.len()
    }
}

/// 明文封装
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Plaintext {
    pub data: Vec<u8>,
}

impl Plaintext {
    pub fn new(data: Vec<u8>) -> Self {
        Self { data }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        Self {
            data: s.as_bytes().to_vec(),
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn to_string_lossy(&self) -> String {
        String::from_utf8_lossy(&self.data).to_string()
    }
}

impl From<String> for Plaintext {
    fn from(s: String) -> Self {
        Self {
            data: s.into_bytes(),
        }
    }
}

impl From<&str> for Plaintext {
    fn from(s: &str) -> Self {
        Self::from_str(s)
    }
}

impl From<Vec<u8>> for Plaintext {
    fn from(v: Vec<u8>) -> Self {
        Self { data: v }
    }
}

/// AES-256-GCM 加密器
#[derive(Debug, Default)]
pub struct AesGcmCipher;

impl AesGcmCipher {
    pub fn new() -> Self {
        Self
    }

    /// 使用 DEK 加密明文，返回 [`Ciphertext`]
    pub fn encrypt(&self, plaintext: &Plaintext, dek: &WorkspaceDek) -> Result<Ciphertext, Error> {
        let key = Key::<Aes256Gcm>::from_slice(dek.as_bytes());
        let cipher = Aes256Gcm::new(key);

        let mut nonce_bytes = [0u8; NONCE_LEN];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|e| Error::Encryption(format!("aes-gcm encrypt failed: {}", e)))?;

        debug!(
            "encrypted {} plaintext bytes -> {} ciphertext bytes",
            plaintext.len(),
            ciphertext.len()
        );
        Ok(Ciphertext {
            nonce: nonce_bytes.to_vec(),
            ciphertext,
        })
    }

    /// 使用 DEK 解密密文，返回 [`Plaintext`]
    pub fn decrypt(&self, ciphertext: &Ciphertext, dek: &WorkspaceDek) -> Result<Plaintext, Error> {
        if ciphertext.nonce.len() != NONCE_LEN {
            return Err(Error::InvalidInput(format!(
                "invalid nonce length: {} != {}",
                ciphertext.nonce.len(),
                NONCE_LEN
            )));
        }
        let key = Key::<Aes256Gcm>::from_slice(dek.as_bytes());
        let cipher = Aes256Gcm::new(key);
        let nonce = Nonce::from_slice(&ciphertext.nonce);

        let plaintext = cipher
            .decrypt(nonce, ciphertext.ciphertext.as_ref())
            .map_err(|e| {
                warn!("aes-gcm decrypt failed: {}", e);
                Error::Decryption(format!("aes-gcm decrypt failed: {}", e))
            })?;

        Ok(Plaintext::new(plaintext))
    }

    /// 便捷：加密字符串明文
    pub fn encrypt_str(&self, plaintext: &str, dek: &WorkspaceDek) -> Result<Ciphertext, Error> {
        self.encrypt(&Plaintext::from_str(plaintext), dek)
    }
}

/// 顶层便捷函数：使用 DEK 加密字节切片
pub fn encrypt(plaintext: &[u8], dek: &WorkspaceDek) -> Result<Ciphertext, Error> {
    AesGcmCipher::new().encrypt(&Plaintext::new(plaintext.to_vec()), dek)
}

/// 顶层便捷函数：使用 DEK 解密密文
pub fn decrypt(ciphertext: &Ciphertext, dek: &WorkspaceDek) -> Result<Plaintext, Error> {
    AesGcmCipher::new().decrypt(ciphertext, dek)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_dek() -> WorkspaceDek {
        WorkspaceDek::new("ws-test").unwrap()
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let dek = make_dek();
        let cipher = AesGcmCipher::new();
        let pt = Plaintext::from_str("hello aurora e2ee");
        let ct = cipher.encrypt(&pt, &dek).unwrap();
        let recovered = cipher.decrypt(&ct, &dek).unwrap();
        assert_eq!(recovered, pt);
    }

    #[test]
    fn test_encrypt_str_roundtrip() {
        let dek = make_dek();
        let cipher = AesGcmCipher::new();
        let ct = cipher.encrypt_str("秘密笔记内容", &dek).unwrap();
        let pt = cipher.decrypt(&ct, &dek).unwrap();
        assert_eq!(pt.to_string_lossy(), "秘密笔记内容");
    }

    #[test]
    fn test_encrypt_top_level_helpers() {
        let dek = make_dek();
        let ct = encrypt(b"raw bytes", &dek).unwrap();
        let pt = decrypt(&ct, &dek).unwrap();
        assert_eq!(pt.as_bytes(), b"raw bytes");
    }

    #[test]
    fn test_wrong_key_fails() {
        let dek1 = WorkspaceDek::new("ws-1").unwrap();
        let dek2 = WorkspaceDek::new("ws-2").unwrap();
        let cipher = AesGcmCipher::new();
        let ct = cipher.encrypt_str("data", &dek1).unwrap();
        let result = cipher.decrypt(&ct, &dek2);
        assert!(result.is_err(), "decryption with wrong key must fail");
    }

    #[test]
    fn test_tampered_ciphertext_fails() {
        let dek = make_dek();
        let cipher = AesGcmCipher::new();
        let ct = cipher.encrypt_str("integrity check", &dek).unwrap();

        let mut tampered = ct.clone();
        if !tampered.ciphertext.is_empty() {
            tampered.ciphertext[0] ^= 0xFF;
        }
        assert!(
            cipher.decrypt(&tampered, &dek).is_err(),
            "tampered ciphertext must fail GCM tag"
        );
    }

    #[test]
    fn test_tampered_nonce_fails() {
        let dek = make_dek();
        let cipher = AesGcmCipher::new();
        let ct = cipher.encrypt_str("nonce check", &dek).unwrap();

        let mut tampered = ct.clone();
        tampered.nonce[0] ^= 0xFF;
        assert!(cipher.decrypt(&tampered, &dek).is_err());
    }

    #[test]
    fn test_empty_plaintext_roundtrip() {
        let dek = make_dek();
        let cipher = AesGcmCipher::new();
        let pt = Plaintext::new(Vec::new());
        let ct = cipher.encrypt(&pt, &dek).unwrap();
        // 密文应至少包含 tag
        assert!(ct.ciphertext.len() >= TAG_LEN);
        let recovered = cipher.decrypt(&ct, &dek).unwrap();
        assert_eq!(recovered, pt);
        assert!(recovered.is_empty());
    }

    #[test]
    fn test_large_plaintext_roundtrip() {
        let dek = make_dek();
        let cipher = AesGcmCipher::new();
        let data = vec![0x55u8; 64 * 1024];
        let pt = Plaintext::new(data.clone());
        let ct = cipher.encrypt(&pt, &dek).unwrap();
        let recovered = cipher.decrypt(&ct, &dek).unwrap();
        assert_eq!(recovered.as_bytes(), &data);
    }

    #[test]
    fn test_ciphertext_to_from_bytes_roundtrip() {
        let dek = make_dek();
        let cipher = AesGcmCipher::new();
        let ct = cipher.encrypt_str("serialize me", &dek).unwrap();
        let bytes = ct.to_bytes();
        assert_eq!(bytes.len(), ct.total_len());
        let ct2 = Ciphertext::from_bytes(&bytes).unwrap();
        assert_eq!(ct, ct2);
        let pt = cipher.decrypt(&ct2, &dek).unwrap();
        assert_eq!(pt.to_string_lossy(), "serialize me");
    }

    #[test]
    fn test_ciphertext_from_bytes_too_short() {
        let result = Ciphertext::from_bytes(&[0u8; 5]);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_nonce_length_rejected() {
        let dek = make_dek();
        let cipher = AesGcmCipher::new();
        let bad = Ciphertext::new(vec![0u8; 11], vec![0u8; 32]);
        assert!(cipher.decrypt(&bad, &dek).is_err());
    }

    #[test]
    fn test_plaintext_helpers() {
        let pt = Plaintext::from_str("abc");
        assert_eq!(pt.len(), 3);
        assert!(!pt.is_empty());
        assert_eq!(pt.to_string_lossy(), "abc");
        let pt2: Plaintext = "xyz".into();
        assert_eq!(pt2.as_bytes(), b"xyz");
        let pt3: Plaintext = vec![1, 2, 3].into();
        assert_eq!(pt3.as_bytes(), &[1, 2, 3]);
    }

    #[test]
    fn test_each_encryption_uses_fresh_nonce() {
        let dek = make_dek();
        let cipher = AesGcmCipher::new();
        let ct1 = cipher.encrypt_str("same", &dek).unwrap();
        let ct2 = cipher.encrypt_str("same", &dek).unwrap();
        assert_ne!(ct1.nonce, ct2.nonce, "nonce must be unique per encryption");
        assert_ne!(ct1.ciphertext, ct2.ciphertext);
    }
}
