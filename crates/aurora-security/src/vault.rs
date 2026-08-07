//! 本地 DEK 保险库（过渡方案）
//!
//! 平台适配层在未接入「口令解锁 + [`KeyHierarchy`]」之前的密钥存储：
//! - 首启生成 32 字节随机 DEK，保存为 `dek.bin`（Unix 下 0600 权限）；
//! - 笔记明文经 [`CryptoProvider`]（AES-256-GCM）加密后落库，满足
//!   V19 §10.2.2「所有模块经 CryptoProvider 访问密码学能力」的依赖规则；
//! - **迁移路径**：生产接入用户口令后，由 `KeyHierarchy::unlock(password)`
//!   派生主密钥包裹 DEK，并将 wrapped DEK 交由 OS 安全存储
//!   （Windows DPAPI / macOS Keychain / Linux Secret Service）保管。

use std::path::{Path, PathBuf};

use rand::{rngs::OsRng, RngCore};

use aurora_core::traits::crypto_provider::{Ciphertext, CryptoProvider};

use crate::Error;

/// 本地文件 DEK 保险库。
#[derive(Debug)]
pub struct LocalDekVault {
    dek: [u8; 32],
    path: PathBuf,
}

impl LocalDekVault {
    /// 加载已有 DEK 文件；不存在则生成并写入。
    ///
    /// # Errors
    /// - 已存在但长度非 32 字节（文件损坏）时返回 [`Error::InvalidInput`]。
    pub fn load_or_create(keys_dir: &Path) -> Result<Self, Error> {
        std::fs::create_dir_all(keys_dir).map_err(Error::Io)?;
        let path = keys_dir.join("dek.bin");
        let dek = if path.exists() {
            let raw = std::fs::read(&path).map_err(Error::Io)?;
            if raw.len() != 32 {
                return Err(Error::InvalidInput(format!(
                    "vault file corrupt: expected 32 bytes, got {}",
                    raw.len()
                )));
            }
            let mut dek = [0u8; 32];
            dek.copy_from_slice(&raw);
            dek
        } else {
            let mut dek = [0u8; 32];
            OsRng.fill_bytes(&mut dek);
            write_vault_file(&path, &dek)?;
            dek
        };
        Ok(Self { dek, path })
    }

    /// 保险库文件路径（用于诊断/迁移）。
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// DEK 字节（供需要直接访问密钥的场景，如 KeyHierarchy 迁移）。
    pub fn dek(&self) -> &[u8; 32] {
        &self.dek
    }

    /// 加密明文并序列化为可落库字节（bincode(Ciphertext)）。
    pub fn encrypt(&self, crypto: &dyn CryptoProvider, plaintext: &[u8]) -> Result<Vec<u8>, Error> {
        let ct = crypto
            .encrypt(plaintext, &self.dek)
            .map_err(|e| Error::Encryption(e.to_string()))?;
        bincode::serialize(&ct).map_err(Error::Bincode)
    }

    /// 反序列化并解密（认证失败返回 [`Error::Decryption`]）。
    pub fn decrypt(&self, crypto: &dyn CryptoProvider, data: &[u8]) -> Result<Vec<u8>, Error> {
        let ct: Ciphertext = bincode::deserialize(data).map_err(Error::Bincode)?;
        crypto
            .decrypt(&ct, &self.dek)
            .map_err(|e| Error::Decryption(e.to_string()))
    }
}

/// Unix 下以 0600 权限写入（仅属主可读写）。
#[cfg(unix)]
fn write_vault_file(path: &Path, dek: &[u8; 32]) -> Result<(), Error> {
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

    let mut file = std::fs::File::create(path).map_err(Error::Io)?;
    file.write_all(dek).map_err(Error::Io)?;
    file.set_permissions(std::fs::Permissions::from_mode(0o600))
        .map_err(Error::Io)
}

/// 非 Unix 平台（Windows 依赖用户目录 ACL 保护）。
#[cfg(not(unix))]
fn write_vault_file(path: &Path, dek: &[u8; 32]) -> Result<(), Error> {
    use std::io::Write;

    let mut file = std::fs::File::create(path).map_err(Error::Io)?;
    file.write_all(dek).map_err(Error::Io)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto_provider_impl::SecurityCryptoProvider;

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let vault = LocalDekVault::load_or_create(dir.path()).unwrap();
        let crypto = SecurityCryptoProvider::new();

        let data = b"note content with secrets";
        let sealed = vault.encrypt(&crypto, data).unwrap();
        assert_ne!(sealed, data);
        let opened = vault.decrypt(&crypto, &sealed).unwrap();
        assert_eq!(opened, data);
    }

    #[test]
    fn reload_same_key() {
        let dir = tempfile::tempdir().unwrap();
        let v1 = LocalDekVault::load_or_create(dir.path()).unwrap();
        let v2 = LocalDekVault::load_or_create(dir.path()).unwrap();
        assert_eq!(v1.dek(), v2.dek(), "reload must reuse persisted DEK");
    }

    #[test]
    fn corrupt_vault_rejected() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("dek.bin"), b"too-short").unwrap();
        let err = LocalDekVault::load_or_create(dir.path()).unwrap_err();
        assert!(matches!(err, Error::InvalidInput(_)));
    }

    #[test]
    fn tampered_ciphertext_fails_auth() {
        let dir = tempfile::tempdir().unwrap();
        let vault = LocalDekVault::load_or_create(dir.path()).unwrap();
        let crypto = SecurityCryptoProvider::new();

        let sealed = vault.encrypt(&crypto, b"hello").unwrap();
        let mut tampered = sealed.clone();
        let last = tampered.len() - 1;
        tampered[last] ^= 0xFF;
        assert!(vault.decrypt(&crypto, &tampered).is_err());
    }
}
