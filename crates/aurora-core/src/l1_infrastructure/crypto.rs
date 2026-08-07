//! 密码学 (ring / aes-gcm / argon2 / sha3)
//!
//! 提供端到端加密、密钥派生、哈希与签名等密码学能力，保障数据安全。
//!
//! - [ring](https://briansmith.org/rustdoc/ring/)：底层密码学原语
//! - [aes-gcm](https://docs.rs/aes-gcm)：AES-GCM 对称加密
//! - [argon2](https://docs.rs/argon2)：Argon2 密钥派生 / 口令哈希
//! - [sha3](https://docs.rs/sha3)：SHA-3 哈希族

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use sha3::{Digest, Sha3_256};

/// 密码学上下文，封装常用密码学配置。
#[derive(Default)]
pub struct CryptoContext {
    pub argon2_params: argon2::Params,
}

impl CryptoContext {
    /// 创建默认密码学上下文。
    pub fn new() -> Self {
        Self::default()
    }
}

/// 生成指定长度的安全随机字节串。
pub fn generate_random_bytes(len: usize) -> Result<Vec<u8>, crate::Error> {
    let mut buf = vec![0u8; len];
    ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut buf)
        .map_err(|e| crate::Error::Internal(format!("random generation failed: {}", e)))?;
    Ok(buf)
}

/// 使用 SHA3-256 计算数据哈希。
pub fn sha3_256(data: &[u8]) -> Vec<u8> {
    let mut hasher = Sha3_256::new();
    hasher.update(data);
    hasher.finalize().to_vec()
}

/// 使用 Argon2id 从口令派生密钥。
///
/// 返回 `(hash_string, salt)`，其中 `hash_string` 可用于后续验证。
pub fn derive_key_from_password(
    password: &str,
    salt: Option<&[u8]>,
) -> Result<(String, Vec<u8>), crate::Error> {
    let salt = match salt {
        Some(s) => s.to_vec(),
        None => generate_random_bytes(16)?,
    };
    let argon2 = Argon2::default();
    let salt_string = argon2::password_hash::SaltString::encode_b64(&salt)
        .map_err(|e| crate::Error::Internal(format!("salt encoding failed: {}", e)))?;
    let password_hash = argon2
        .hash_password(password.as_bytes(), &salt_string)
        .map_err(|e| crate::Error::Internal(format!("argon2 hash failed: {}", e)))?;
    Ok((password_hash.to_string(), salt))
}

/// 验证口令是否与 Argon2 哈希匹配。
pub fn verify_password(password: &str, hash: &str) -> Result<bool, crate::Error> {
    let parsed_hash = PasswordHash::new(hash)
        .map_err(|e| crate::Error::Internal(format!("password hash parse failed: {}", e)))?;
    let argon2 = Argon2::default();
    Ok(argon2
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
}

/// 使用 AES-256-GCM 加密数据。
///
/// # Arguments
/// * `plaintext` — 待加密明文。
/// * `key` — 32 字节对称密钥。
/// * `nonce` — 12 字节 nonce；若为 None 则自动生成。
///
/// 返回 `(ciphertext, nonce)`。
pub fn encrypt_aes_gcm(
    plaintext: &[u8],
    key: &[u8],
    nonce: Option<&[u8]>,
) -> Result<(Vec<u8>, Vec<u8>), crate::Error> {
    if key.len() != 32 {
        return Err(crate::Error::InvalidInput(format!(
            "AES-256-GCM key must be 32 bytes, got {}",
            key.len()
        )));
    }
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| crate::Error::Internal(format!("aes key init failed: {}", e)))?;
    let nonce_vec = match nonce {
        Some(n) => n.to_vec(),
        None => generate_random_bytes(12)?,
    };
    let nonce = Nonce::from_slice(&nonce_vec);
    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| crate::Error::Internal(format!("aes encryption failed: {}", e)))?;
    Ok((ciphertext, nonce_vec))
}

/// 使用 AES-256-GCM 解密数据。
pub fn decrypt_aes_gcm(
    ciphertext: &[u8],
    key: &[u8],
    nonce: &[u8],
) -> Result<Vec<u8>, crate::Error> {
    if key.len() != 32 {
        return Err(crate::Error::InvalidInput(format!(
            "AES-256-GCM key must be 32 bytes, got {}",
            key.len()
        )));
    }
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| crate::Error::Internal(format!("aes key init failed: {}", e)))?;
    let nonce = Nonce::from_slice(nonce);
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| crate::Error::Internal(format!("aes decryption failed: {}", e)))?;
    Ok(plaintext)
}

/// 使用 ring 的 ECDSA (P-256) 对数据进行签名。
///
/// # Arguments
/// * `data` — 待签名数据。
/// * `private_key` — PKCS#8 编码的私钥。
///
/// 返回签名字节串。
pub fn sign_ecdsa_p256(data: &[u8], private_key: &[u8]) -> Result<Vec<u8>, crate::Error> {
    let key_pair = ring::signature::EcdsaKeyPair::from_pkcs8(
        &ring::signature::ECDSA_P256_SHA256_ASN1_SIGNING,
        private_key,
        &ring::rand::SystemRandom::new(),
    )
    .map_err(|e| crate::Error::Internal(format!("ecdsa key pair init failed: {}", e)))?;
    let signature = key_pair
        .sign(&ring::rand::SystemRandom::new(), data)
        .map_err(|e| crate::Error::Internal(format!("ecdsa sign failed: {}", e)))?;
    Ok(signature.as_ref().to_vec())
}

/// 使用 ring 的 ECDSA (P-256) 验证签名。
pub fn verify_ecdsa_p256(
    data: &[u8],
    public_key: &[u8],
    signature: &[u8],
) -> Result<bool, crate::Error> {
    let public_key = ring::signature::UnparsedPublicKey::new(
        &ring::signature::ECDSA_P256_SHA256_ASN1,
        public_key,
    );
    Ok(public_key.verify(data, signature).is_ok())
}

/// base64 编码辅助模块（简化实现，避免额外依赖）。
mod base64 {
    pub fn encode(input: &[u8]) -> String {
        const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
        for chunk in input.chunks(3) {
            let b = ((chunk[0] as u32) << 16)
                | ((chunk.get(1).copied().unwrap_or(0) as u32) << 8)
                | (chunk.get(2).copied().unwrap_or(0) as u32);
            out.push(CHARS[((b >> 18) & 0x3F) as usize] as char);
            out.push(CHARS[((b >> 12) & 0x3F) as usize] as char);
            out.push(if chunk.len() > 1 {
                CHARS[((b >> 6) & 0x3F) as usize] as char
            } else {
                '='
            });
            out.push(if chunk.len() > 2 {
                CHARS[(b & 0x3F) as usize] as char
            } else {
                '='
            });
        }
        out
    }

    pub fn decode(input: &str) -> Result<Vec<u8>, crate::Error> {
        // 标准 base64 输入长度必须为 4 的倍数（含 padding）。非 4 倍数时
        // `chunks(4)` 的末块不足 4 字节，后续下标访问会越界 panic，
        // 这里先校验并返回错误，避免崩溃。
        if !input.is_empty() && input.len() % 4 != 0 {
            return Err(crate::Error::InvalidInput(format!(
                "invalid base64 length: {} (must be a multiple of 4)",
                input.len()
            )));
        }
        let mut out = Vec::with_capacity(input.len() / 4 * 3);
        for chunk in input.as_bytes().chunks(4) {
            let decode_char = |c: u8| -> Option<u8> {
                match c {
                    b'A'..=b'Z' => Some(c - b'A'),
                    b'a'..=b'z' => Some(c - b'a' + 26),
                    b'0'..=b'9' => Some(c - b'0' + 52),
                    b'+' => Some(62),
                    b'/' => Some(63),
                    b'=' => Some(0),
                    _ => None,
                }
            };
            let a = decode_char(chunk[0])
                .ok_or_else(|| crate::Error::InvalidInput("invalid base64".to_string()))?;
            let b = decode_char(chunk[1])
                .ok_or_else(|| crate::Error::InvalidInput("invalid base64".to_string()))?;
            let c = decode_char(chunk[2])
                .ok_or_else(|| crate::Error::InvalidInput("invalid base64".to_string()))?;
            let d = decode_char(chunk[3])
                .ok_or_else(|| crate::Error::InvalidInput("invalid base64".to_string()))?;
            out.push((a << 2) | (b >> 4));
            if chunk[2] != b'=' {
                out.push((b << 4) | (c >> 2));
            }
            if chunk[3] != b'=' {
                out.push((c << 6) | d);
            }
        }
        Ok(out)
    }
}

pub use base64::{decode as base64_decode, encode as base64_encode};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_roundtrip() {
        for data in [
            b"".as_slice(),
            b"a",
            b"ab",
            b"abc",
            b"hello world",
            b"\x00\x01\x02\xff\xfe",
        ] {
            let encoded = base64_encode(data);
            let decoded = base64_decode(&encoded).unwrap();
            assert_eq!(decoded, data, "roundtrip failed for {:?}", data);
        }
    }

    #[test]
    fn base64_known_vectors() {
        assert_eq!(base64_encode(b"hello"), "aGVsbG8=");
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_decode("aGVsbG8=").unwrap(), b"hello");
    }

    #[test]
    fn base64_decode_rejects_non_multiple_of_4() {
        // 长度非 4 倍数（缺 padding）必须返回错误而非 panic
        let err = base64_decode("aGVsbG8").unwrap_err();
        assert!(matches!(err, crate::Error::InvalidInput(_)));
        // 空输入合法
        assert_eq!(base64_decode("").unwrap(), b"");
    }

    #[test]
    fn base64_decode_rejects_invalid_chars() {
        let err = base64_decode("ab!=").unwrap_err();
        assert!(matches!(err, crate::Error::InvalidInput(_)));
    }
}
