//! Aurora 安全与加密系统 (Aurora Security)
//!
//! 端到端加密、密钥层次结构、后量子混合密钥交换、生物识别保护与密钥恢复。
//!
//! # 模块总览
//! - [`key_hierarchy`]：用户口令 → Argon2id 主密钥 → 工作区 DEK
//! - [`e2ee`]：AES-256-GCM 端到端加密（零知识架构）
//! - [`post_quantum`]：ML-KEM-768 + X25519 双轨混合密钥交换
//! - [`biometric`]：FaceID/TouchID/TPM/Secure Enclave 生物识别保护
//! - [`recovery`]：BIP39 助记词 + Shamir 秘密分享 + 设备授权二维码

pub mod biometric;
pub mod crypto_provider_impl;
pub mod e2ee;
pub mod key_hierarchy;
pub mod post_quantum;
pub mod recovery;

use thiserror::Error;

/// Aurora 安全层统一错误类型。
#[derive(Error, Debug)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Bincode error: {0}")]
    Bincode(#[from] bincode::Error),
    #[error("Key derivation error: {0}")]
    Kdf(String),
    #[error("Encryption error: {0}")]
    Encryption(String),
    #[error("Decryption error: {0}")]
    Decryption(String),
    #[error("Key exchange error: {0}")]
    KeyExchange(String),
    #[error("Biometric error: {0}")]
    Biometric(String),
    #[error("Recovery error: {0}")]
    Recovery(String),
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Internal error: {0}")]
    Internal(String),
}

// ---- 公共再导出：常用类型可直接从 crate 根访问 ----

pub use biometric::{
    BiometricAuthenticator, BiometricConfig, BiometricKind, BiometricProtector, BiometricStatus,
    MockBiometricAuthenticator,
};
pub use e2ee::{encrypt, decrypt, AesGcmCipher, Ciphertext, Plaintext};
pub use key_hierarchy::{KeyHierarchy, MasterKey, WorkspaceDek};
pub use post_quantum::{
    HybridEncapsulation, HybridKeyExchange, HybridKeyPair, KemAlgorithm, KemKeyPair, MockKem,
    PostQuantumKem,
};
pub use recovery::{DeviceAuthorizationQr, Mnemonic, ShamirSecretSharing, ShamirShare};

#[cfg(test)]
mod tests {
    //! 跨模块集成测试：验证密钥派生 → 加密 → 恢复 的完整链路。

    use super::*;
    use e2ee::AesGcmCipher;

    #[test]
    fn test_full_pipeline_master_key_to_dek_to_e2ee() {
        // 1. 用户口令派生主密钥
        let kh = KeyHierarchy::new();
        kh.unlock("user-strong-password").unwrap();

        // 2. 工作区 DEK
        let dek = kh.create_dek("workspace-1").unwrap();

        // 3. 端到端加密
        let cipher = AesGcmCipher::new();
        let ct = cipher.encrypt_str("敏感笔记内容", &dek).unwrap();
        let pt = cipher.decrypt(&ct, &dek).unwrap();
        assert_eq!(pt.to_string_lossy(), "敏感笔记内容");
    }

    #[test]
    fn test_recovery_pipeline_mnemonic_and_shamir() {
        // 1. 生成主密钥并派生 DEK
        let kh = KeyHierarchy::new();
        let mk = kh.unlock("recoverable-password").unwrap();

        // 2. 将主密钥拆为 3 份 Shamir 份额
        let sss = ShamirSecretSharing::default_3_of_2();
        let shares = sss.split(mk.as_bytes()).unwrap();

        // 3. 用其中 2 份重建主密钥
        let recon = sss.combine(&[shares[0].clone(), shares[2].clone()]).unwrap();
        assert_eq!(recon, mk.as_bytes());
    }

    #[test]
    fn test_mnemonic_can_seed_a_dek() {
        // 助记词派生种子，再取前 32 字节作为 DEK，验证可加解密
        let m = Mnemonic::generate().unwrap();
        let seed = m.to_seed(None);
        let dek = WorkspaceDek::from_bytes("ws-from-mnemonic", {
            let mut k = [0u8; 32];
            k.copy_from_slice(&seed[..32]);
            k
        });
        let cipher = AesGcmCipher::new();
        let ct = cipher.encrypt_str("from mnemonic", &dek).unwrap();
        let pt = cipher.decrypt(&ct, &dek).unwrap();
        assert_eq!(pt.to_string_lossy(), "from mnemonic");
    }

    #[test]
    fn test_hybrid_exchange_derives_shared_dek() {
        // 两条轨道协商出同一最终密钥，可作为对称 DEK 使用
        let ex = HybridKeyExchange::with_mock();
        let recipient = ex.generate_keypair().unwrap();
        let enc = ex.encapsulate(&recipient).unwrap();
        let shared = ex.decapsulate(&recipient, &enc).unwrap();

        let dek = WorkspaceDek::from_bytes("ws-hybrid", {
            let mut k = [0u8; 32];
            k.copy_from_slice(&shared);
            k
        });
        let cipher = AesGcmCipher::new();
        let ct = cipher.encrypt_str("hybrid secret", &dek).unwrap();
        let pt = cipher.decrypt(&ct, &dek).unwrap();
        assert_eq!(pt.to_string_lossy(), "hybrid secret");
    }
}
