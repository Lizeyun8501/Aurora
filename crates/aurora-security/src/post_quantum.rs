//! 后量子加密 (Post-Quantum Cryptography)
//!
//! ML-KEM-768 + X25519 双轨并行混合密钥交换：
//! - 经典轨道：X25519（抗现有计算能力）
//! - 后量子轨道：ML-KEM-768（抗量子攻击）
//! - 最终密钥 = KDF(X25519_shared || ML-KEM_shared)
//!
//! # 实现说明
//! 本模块提供 [`PostQuantumKem`] trait 与 [`HybridKeyExchange`] 编排器。
//! - 经典轨道：[`MockKem`]（基于 `ring` 的 X25519 ECDH）
//! - 后量子轨道：[`MlKem768Kem`]（基于 `libcrux-ml-kem`，FIPS 203 标准化）
//!
//! 生产环境使用 [`HybridKeyExchange::with_real_ml_kem`] 或 [`Default::default()`]，
//! 测试环境使用 [`HybridKeyExchange::with_mock`]（双 X25519 占位）。
//!
//! 混合共享秘密的 KDF 使用 HKDF-SHA3-256（基于 `hkdf` + `sha3`）；
//! 如需严格 HKDF-SHA256，可将 digest 切换为 `sha2::Sha256`。

use std::collections::HashMap;
use std::sync::Arc;

use hkdf::Hkdf;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use sha3::Sha3_256;
use tracing::{debug, info, warn};

use crate::Error;

/// 混合 KDF 派生输出长度（256-bit）
pub const HYBRID_KEY_LEN: usize = 32;

/// KEM 密钥对
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KemKeyPair {
    /// 公钥（可公开传输）
    pub public: Vec<u8>,
    /// 私钥句柄（实现相关）。
    /// 在 `MockKem` 中为公钥副本，用于在内部查找缓存的临时私钥。
    pub private: Vec<u8>,
}

/// 封装结果：密文 + 共享秘密
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Encapsulation {
    /// 密文（发送给对方用于解封装）
    pub ciphertext: Vec<u8>,
    /// 协商出的共享秘密
    pub shared_secret: Vec<u8>,
}

/// KEM 算法标识，支持迁移
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum KemAlgorithm {
    /// 经典 X25519
    X25519,
    /// 后量子 ML-KEM-768
    MlKem768,
    /// Mock 占位（双 X25519）
    MockX25519,
}

impl KemAlgorithm {
    /// 是否后量子安全
    pub fn is_post_quantum(&self) -> bool {
        matches!(self, KemAlgorithm::MlKem768)
    }
}

/// 后量子 KEM trait
///
/// 一个 KEM 提供三种操作：
/// 1. `generate_keypair` —— 接收方生成长期密钥对；
/// 2. `encapsulate(peer_public)` —— 发送方用对方公钥生成共享秘密与密文；
/// 3. `decapsulate(private, ciphertext)` —— 接收方用私钥与密文恢复共享秘密。
pub trait PostQuantumKem {
    /// 算法名称
    fn name(&self) -> &str;
    /// 算法标识
    fn algorithm(&self) -> KemAlgorithm;
    /// 是否后量子安全
    fn is_post_quantum(&self) -> bool {
        self.algorithm().is_post_quantum()
    }
    /// 生成密钥对
    fn generate_keypair(&self) -> Result<KemKeyPair, Error>;
    /// 封装
    fn encapsulate(&self, peer_public: &[u8]) -> Result<Encapsulation, Error>;
    /// 解封装
    fn decapsulate(&self, private: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, Error>;
}

/// 使用 X25519（来自 `ring`）的 Mock KEM —— 后量子占位实现。
///
/// - 封装：生成临时 X25519 密钥对，与接收方公钥执行 ECDH 得到共享秘密，
///   密文 = 临时公钥。
/// - 解封装：使用接收方私钥与密文（临时公钥）执行 ECDH 恢复共享秘密。
///
/// 由于 `ring` 的 agreement API 仅暴露临时私钥（不可序列化、不可复用），
/// `MockKem` 在内部以公钥为句柄缓存接收方临时私钥；每个密钥对为一次性
/// （解封装后即从内部缓存移除）。真实静态 X25519 / ML-KEM 实现可携带可
/// 序列化私钥，无需此限制。
pub struct MockKem {
    keys: Arc<Mutex<HashMap<Vec<u8>, ring::agreement::EphemeralPrivateKey>>>,
}

impl MockKem {
    pub fn new() -> Self {
        Self {
            keys: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl Default for MockKem {
    fn default() -> Self {
        Self::new()
    }
}

impl PostQuantumKem for MockKem {
    fn name(&self) -> &str {
        "MockX25519Kem"
    }

    fn algorithm(&self) -> KemAlgorithm {
        KemAlgorithm::MockX25519
    }

    fn generate_keypair(&self) -> Result<KemKeyPair, Error> {
        let rng = ring::rand::SystemRandom::new();
        let priv_key =
            ring::agreement::EphemeralPrivateKey::generate(&ring::agreement::X25519, &rng)
                .map_err(|_| Error::KeyExchange("x25519 keygen failed".into()))?;
        let pub_key = priv_key
            .compute_public_key()
            .map_err(|_| Error::KeyExchange("x25519 public key derivation failed".into()))?;
        let pub_bytes = pub_key.as_ref().to_vec();
        self.keys.lock().insert(pub_bytes.clone(), priv_key);
        debug!("generated mock X25519 keypair");
        Ok(KemKeyPair {
            public: pub_bytes.clone(),
            private: pub_bytes,
        })
    }

    fn encapsulate(&self, peer_public: &[u8]) -> Result<Encapsulation, Error> {
        let rng = ring::rand::SystemRandom::new();
        let ephemeral =
            ring::agreement::EphemeralPrivateKey::generate(&ring::agreement::X25519, &rng)
                .map_err(|_| Error::KeyExchange("x25519 ephemeral keygen failed".into()))?;
        let ephemeral_pub = ephemeral
            .compute_public_key()
            .map_err(|_| Error::KeyExchange("x25519 ephemeral public key failed".into()))?;
        let ciphertext = ephemeral_pub.as_ref().to_vec();

        let shared = x25519_agree(ephemeral, peer_public)
            .map_err(|e| Error::KeyExchange(format!("encapsulate agree failed: {}", e)))?;

        Ok(Encapsulation {
            ciphertext,
            shared_secret: shared,
        })
    }

    fn decapsulate(&self, private: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, Error> {
        let priv_key = self
            .keys
            .lock()
            .remove(private)
            .ok_or_else(|| Error::KeyExchange("no stored private key for handle".into()))?;
        x25519_agree(priv_key, ciphertext)
            .map_err(|e| Error::KeyExchange(format!("decapsulate agree failed: {}", e)))
    }
}

// ── ML-KEM-768 真实实现（libcrux-ml-kem） ─────────────────────

/// 基于 `libcrux-ml-kem` 的 ML-KEM-768 (FIPS 203) 真实实现。
///
/// # 安全性
///
/// - 使用 `rand::thread_rng()` 提供 `CryptoRng` 级随机源
/// - 密钥对可序列化存储（公钥 1184 字节，私钥 2400 字节）
/// - 密文 1088 字节，共享秘密 32 字节
/// - 通过 FO 变换保证 IND-CCA2 安全性
pub struct MlKem768Kem;

impl MlKem768Kem {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MlKem768Kem {
    fn default() -> Self {
        Self
    }
}

impl PostQuantumKem for MlKem768Kem {
    fn name(&self) -> &str {
        "ML-KEM-768 (libcrux)"
    }

    fn algorithm(&self) -> KemAlgorithm {
        KemAlgorithm::MlKem768
    }

    fn generate_keypair(&self) -> Result<KemKeyPair, Error> {
        use libcrux_ml_kem::mlkem768;
        use rand::RngCore;

        let mut randomness = [0u8; 64];
        rand::thread_rng().fill_bytes(&mut randomness);

        let kp = mlkem768::generate_key_pair(randomness);
        let pk = kp.public_key().as_slice().to_vec();
        let sk = kp.private_key().as_slice().to_vec();

        debug!(
            pk_len = pk.len(),
            sk_len = sk.len(),
            "generated ML-KEM-768 keypair"
        );

        Ok(KemKeyPair {
            public: pk,
            private: sk,
        })
    }

    fn encapsulate(&self, peer_public: &[u8]) -> Result<Encapsulation, Error> {
        use libcrux_ml_kem::mlkem768;
        use rand::RngCore;

        // 从字节切片重建公钥
        let pk_arr: [u8; 1184] = peer_public.try_into().map_err(|_| {
            Error::InvalidInput(format!(
                "ML-KEM-768 public key must be 1184 bytes, got {}",
                peer_public.len()
            ))
        })?;

        let pk = libcrux_ml_kem::mlkem768::MlKem768PublicKey::from(pk_arr);

        let mut randomness = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut randomness);

        let (ct, ss) = mlkem768::encapsulate(&pk, randomness);

        let ciphertext = ct.as_slice().to_vec();
        let shared_secret = ss.as_slice().to_vec();

        debug!(
            ct_len = ciphertext.len(),
            ss_len = shared_secret.len(),
            "ML-KEM-768 encapsulate completed"
        );

        Ok(Encapsulation {
            ciphertext,
            shared_secret,
        })
    }

    fn decapsulate(&self, private: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, Error> {
        use libcrux_ml_kem::mlkem768;

        // 从字节切片重建私钥
        let sk_arr: [u8; 2400] = private.try_into().map_err(|_| {
            Error::InvalidInput(format!(
                "ML-KEM-768 private key must be 2400 bytes, got {}",
                private.len()
            ))
        })?;

        let ct_arr: [u8; 1088] = ciphertext.try_into().map_err(|_| {
            Error::InvalidInput(format!(
                "ML-KEM-768 ciphertext must be 1088 bytes, got {}",
                ciphertext.len()
            ))
        })?;

        let sk = libcrux_ml_kem::mlkem768::MlKem768PrivateKey::from(sk_arr);
        let ct = libcrux_ml_kem::mlkem768::MlKem768Ciphertext::from(ct_arr);

        let ss = mlkem768::decapsulate(&sk, &ct);
        let shared_secret = ss.as_slice().to_vec();

        debug!(
            ss_len = shared_secret.len(),
            "ML-KEM-768 decapsulate completed"
        );

        Ok(shared_secret)
    }
}

/// 使用 ring 临时 X25519 私钥与对方公钥执行 ECDH，返回共享秘密。
fn x25519_agree(
    priv_key: ring::agreement::EphemeralPrivateKey,
    peer_public: &[u8],
) -> Result<Vec<u8>, Error> {
    let peer_pub = ring::agreement::UnparsedPublicKey::new(&ring::agreement::X25519, peer_public);
    ring::agreement::agree_ephemeral(priv_key, &peer_pub, |key_material: &[u8]| {
        Ok::<Vec<u8>, ring::error::Unspecified>(key_material.to_vec())
    })
    .map_err(|_| Error::KeyExchange("x25519 agreement failed".into()))?
    .map_err(|_| Error::KeyExchange("x25519 kdf returned error".into()))
}

/// 接收方混合密钥对
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridKeyPair {
    pub classical: KemKeyPair,
    pub post_quantum: KemKeyPair,
}

/// 发送方封装结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridEncapsulation {
    pub classical: Encapsulation,
    pub post_quantum: Encapsulation,
    /// 派生的最终共享密钥
    pub final_key: Vec<u8>,
}

/// 混合密钥交换编排器：经典轨道 + 后量子轨道并行
pub struct HybridKeyExchange {
    classical_kem: Box<dyn PostQuantumKem>,
    pq_kem: Box<dyn PostQuantumKem>,
}

impl HybridKeyExchange {
    /// 构造自定义两条轨道的混合交换
    pub fn new(classical: Box<dyn PostQuantumKem>, pq: Box<dyn PostQuantumKem>) -> Self {
        Self {
            classical_kem: classical,
            pq_kem: pq,
        }
    }

    /// 使用 [`MockKem`]（双 X25519 占位）构造 — 仅用于测试
    pub fn with_mock() -> Self {
        Self::new(Box::new(MockKem::new()), Box::new(MockKem::new()))
    }

    /// 使用真实 ML-KEM-768 后量子轨道 + X25519 经典轨道构造（生产默认）。
    ///
    /// 对应 V19 §12 安全与加密模块设计：ML-KEM-768 + X25519 双轨并行混合密钥交换。
    /// 后量子轨道使用 `libcrux-ml-kem`（FIPS 203 标准化），经典轨道使用 X25519。
    pub fn with_real_ml_kem() -> Self {
        Self::new(Box::new(MockKem::new()), Box::new(MlKem768Kem::new()))
    }
}

impl Default for HybridKeyExchange {
    /// 生产默认：使用真实 ML-KEM-768 + X25519。
    fn default() -> Self {
        Self::with_real_ml_kem()
    }
}

impl HybridKeyExchange {
    /// 经典轨道算法
    pub fn classical_algorithm(&self) -> KemAlgorithm {
        self.classical_kem.algorithm()
    }

    /// 后量子轨道算法
    pub fn post_quantum_algorithm(&self) -> KemAlgorithm {
        self.pq_kem.algorithm()
    }

    /// 接收方生成混合密钥对
    pub fn generate_keypair(&self) -> Result<HybridKeyPair, Error> {
        let classical = self.classical_kem.generate_keypair()?;
        let post_quantum = self.pq_kem.generate_keypair()?;
        info!(
            "generated hybrid keypair: classical={:?}, pq={:?}",
            self.classical_algorithm(),
            self.post_quantum_algorithm()
        );
        Ok(HybridKeyPair {
            classical,
            post_quantum,
        })
    }

    /// 发送方封装：对两条轨道分别封装，并派生最终密钥
    pub fn encapsulate(&self, peer: &HybridKeyPair) -> Result<HybridEncapsulation, Error> {
        let classical = self.classical_kem.encapsulate(&peer.classical.public)?;
        let post_quantum = self.pq_kem.encapsulate(&peer.post_quantum.public)?;
        let final_key = derive_hybrid_key(&classical.shared_secret, &post_quantum.shared_secret)?;
        Ok(HybridEncapsulation {
            classical,
            post_quantum,
            final_key,
        })
    }

    /// 接收方解封装：恢复两条轨道共享秘密并派生最终密钥
    pub fn decapsulate(
        &self,
        kp: &HybridKeyPair,
        enc: &HybridEncapsulation,
    ) -> Result<Vec<u8>, Error> {
        let classical_ss = self
            .classical_kem
            .decapsulate(&kp.classical.private, &enc.classical.ciphertext)?;
        let pq_ss = self
            .pq_kem
            .decapsulate(&kp.post_quantum.private, &enc.post_quantum.ciphertext)?;
        let final_key = derive_hybrid_key(&classical_ss, &pq_ss)?;
        if final_key != enc.final_key {
            warn!("hybrid decapsulation produced a different final key");
            return Err(Error::KeyExchange(
                "hybrid final key mismatch after decapsulation".into(),
            ));
        }
        Ok(final_key)
    }
}

/// 混合共享秘密 KDF：HKDF-SHA3-256(classical_ss || pq_ss)
pub fn derive_hybrid_key(classical_ss: &[u8], pq_ss: &[u8]) -> Result<Vec<u8>, Error> {
    let mut ikm = Vec::with_capacity(classical_ss.len() + pq_ss.len());
    ikm.extend_from_slice(classical_ss);
    ikm.extend_from_slice(pq_ss);
    let hk = Hkdf::<Sha3_256>::new(Some(b"aurora-hybrid-salt"), &ikm);
    let mut okm = [0u8; HYBRID_KEY_LEN];
    hk.expand(b"aurora-hybrid-kex-v1", &mut okm)
        .map_err(|_| Error::KeyExchange("hkdf expand failed".into()))?;
    Ok(okm.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kem_algorithm_post_quantum_flag() {
        assert!(!KemAlgorithm::X25519.is_post_quantum());
        assert!(KemAlgorithm::MlKem768.is_post_quantum());
        assert!(!KemAlgorithm::MockX25519.is_post_quantum());
    }

    #[test]
    fn test_mock_kem_keypair() {
        let kem = MockKem::new();
        let kp = kem.generate_keypair().unwrap();
        assert_eq!(kp.public.len(), 32, "X25519 public key is 32 bytes");
        assert_eq!(kp.public, kp.private, "mock handle equals public key");
        assert_eq!(kem.name(), "MockX25519Kem");
        assert!(!kem.is_post_quantum());
    }

    #[test]
    fn test_mock_kem_two_keypairs_differ() {
        let kem = MockKem::new();
        let kp1 = kem.generate_keypair().unwrap();
        let kp2 = kem.generate_keypair().unwrap();
        assert_ne!(kp1.public, kp2.public);
    }

    #[test]
    fn test_mock_kem_encapsulate_decapsulate_agreement() {
        let kem = MockKem::new();
        let recipient = kem.generate_keypair().unwrap();
        let enc = kem.encapsulate(&recipient.public).unwrap();

        assert_ne!(enc.shared_secret, Vec::<u8>::new());
        assert_eq!(
            enc.ciphertext.len(),
            32,
            "ciphertext is ephemeral X25519 public key"
        );

        let dec_ss = kem
            .decapsulate(&recipient.private, &enc.ciphertext)
            .unwrap();
        assert_eq!(
            dec_ss, enc.shared_secret,
            "decapsulated secret must match encapsulated"
        );
    }

    #[test]
    fn test_mock_kem_decapsulate_wrong_ciphertext_yields_different_secret() {
        let kem = MockKem::new();
        let recipient = kem.generate_keypair().unwrap();
        // 合法封装，得到正确共享秘密
        let enc = kem.encapsulate(&recipient.public).unwrap();

        // 用另一个无关公钥作为“错误密文”进行解封装。
        // X25519 ECDH 对任意合法公钥都能完成协商（产生不同秘密），
        // 不像带 FO 变换的真实 KEM（如 ML-KEM-768）会在密文不匹配时直接失败。
        // 此处验证：解封装可完成，但得到的秘密与合法封装不同。
        let other = kem.generate_keypair().unwrap();
        let result = kem.decapsulate(&recipient.private, &other.public);
        match result {
            Ok(wrong_secret) => assert_ne!(
                wrong_secret, enc.shared_secret,
                "wrong ciphertext must yield a different shared secret"
            ),
            Err(_) => { /* 对低阶点等异常输入报错亦可接受 */ }
        }
    }

    #[test]
    fn test_mock_kem_unknown_handle_fails() {
        let kem = MockKem::new();
        let result = kem.decapsulate(&[0u8; 32], &[0u8; 32]);
        assert!(result.is_err());
    }

    #[test]
    fn test_mock_kem_encapsulate_invalid_peer_fails() {
        let kem = MockKem::new();
        // 无效公钥（全零的 X25519 公钥在某些实现中会被拒绝；这里至少不应 panic）
        let _ = kem.encapsulate(&[0u8; 32]);
    }

    #[test]
    fn test_hybrid_exchange_agreement() {
        let ex = HybridKeyExchange::with_mock();
        let recipient_kp = ex.generate_keypair().unwrap();
        let enc = ex.encapsulate(&recipient_kp).unwrap();

        assert_eq!(enc.final_key.len(), HYBRID_KEY_LEN);
        assert_ne!(enc.final_key, vec![0u8; HYBRID_KEY_LEN]);

        let dec_key = ex.decapsulate(&recipient_kp, &enc).unwrap();
        assert_eq!(
            dec_key, enc.final_key,
            "sender and receiver must derive the same final key"
        );
    }

    #[test]
    fn test_hybrid_exchange_algorithms() {
        let ex = HybridKeyExchange::with_mock();
        assert_eq!(ex.classical_algorithm(), KemAlgorithm::MockX25519);
        assert_eq!(ex.post_quantum_algorithm(), KemAlgorithm::MockX25519);
    }

    #[test]
    fn test_hybrid_exchange_independent_keys_per_session() {
        let ex = HybridKeyExchange::with_mock();
        let kp_a = ex.generate_keypair().unwrap();
        let enc_a = ex.encapsulate(&kp_a).unwrap();

        let ex2 = HybridKeyExchange::with_mock();
        let kp_b = ex2.generate_keypair().unwrap();
        let enc_b = ex2.encapsulate(&kp_b).unwrap();

        assert_ne!(
            enc_a.final_key, enc_b.final_key,
            "different sessions derive different keys"
        );
    }

    #[test]
    fn test_hybrid_decapsulate_tampered_classical_ciphertext_fails() {
        let ex = HybridKeyExchange::with_mock();
        let kp = ex.generate_keypair().unwrap();
        let mut enc = ex.encapsulate(&kp).unwrap();

        // 篡改经典轨道密文：用一个全新的 X25519 公钥替换
        let kem = MockKem::new();
        let fake = kem.generate_keypair().unwrap();
        enc.classical.ciphertext = fake.public;

        let result = ex.decapsulate(&kp, &enc);
        assert!(
            result.is_err(),
            "tampered classical ciphertext must fail decapsulation"
        );
    }

    #[test]
    fn test_derive_hybrid_key_deterministic() {
        let a = [1u8; 32];
        let b = [2u8; 32];
        let k1 = derive_hybrid_key(&a, &b).unwrap();
        let k2 = derive_hybrid_key(&a, &b).unwrap();
        assert_eq!(k1, k2);
        assert_eq!(k1.len(), HYBRID_KEY_LEN);
    }

    #[test]
    fn test_derive_hybrid_key_order_matters() {
        let a = [1u8; 32];
        let b = [2u8; 32];
        let k1 = derive_hybrid_key(&a, &b).unwrap();
        let k2 = derive_hybrid_key(&b, &a).unwrap();
        assert_ne!(k1, k2, "concatenation order must affect the derived key");
    }

    // ── ML-KEM-768 真实实现测试 ──────────────────────────────

    #[test]
    fn test_mlkem768_keypair_sizes() {
        let kem = MlKem768Kem::new();
        let kp = kem.generate_keypair().unwrap();
        assert_eq!(kp.public.len(), 1184, "ML-KEM-768 public key is 1184 bytes");
        assert_eq!(
            kp.private.len(),
            2400,
            "ML-KEM-768 private key is 2400 bytes"
        );
    }

    #[test]
    fn test_mlkem768_encapsulate_decapsulate_agreement() {
        let kem = MlKem768Kem::new();
        let recipient = kem.generate_keypair().unwrap();
        let enc = kem.encapsulate(&recipient.public).unwrap();

        assert_eq!(
            enc.ciphertext.len(),
            1088,
            "ML-KEM-768 ciphertext is 1088 bytes"
        );
        assert_eq!(
            enc.shared_secret.len(),
            32,
            "ML-KEM-768 shared secret is 32 bytes"
        );
        assert_ne!(
            enc.shared_secret,
            vec![0u8; 32],
            "shared secret must not be all zeros"
        );

        let dec_ss = kem
            .decapsulate(&recipient.private, &enc.ciphertext)
            .unwrap();
        assert_eq!(
            dec_ss, enc.shared_secret,
            "ML-KEM-768 decapsulated secret must match encapsulated"
        );
    }

    #[test]
    fn test_mlkem768_is_post_quantum() {
        let kem = MlKem768Kem::new();
        assert_eq!(kem.name(), "ML-KEM-768 (libcrux)");
        assert_eq!(kem.algorithm(), KemAlgorithm::MlKem768);
        assert!(kem.is_post_quantum());
    }

    #[test]
    fn test_mlkem768_two_keypairs_differ() {
        let kem = MlKem768Kem::new();
        let kp1 = kem.generate_keypair().unwrap();
        let kp2 = kem.generate_keypair().unwrap();
        assert_ne!(
            kp1.public, kp2.public,
            "different keypairs must have different public keys"
        );
        assert_ne!(
            kp1.private, kp2.private,
            "different keypairs must have different private keys"
        );
    }

    #[test]
    fn test_mlkem768_wrong_ciphertext_fails() {
        let kem = MlKem768Kem::new();
        let recipient = kem.generate_keypair().unwrap();
        // 合法封装
        let enc = kem.encapsulate(&recipient.public).unwrap();
        // 篡改密文
        let mut tampered = enc.ciphertext.clone();
        tampered[0] ^= 0xFF;
        // ML-KEM-768 的 FO 变换应检测篡改并产生不同共享秘密
        let wrong_ss = kem.decapsulate(&recipient.private, &tampered).unwrap();
        assert_ne!(
            wrong_ss, enc.shared_secret,
            "tampered ciphertext must yield a different shared secret (FO transform)"
        );
    }

    #[test]
    fn test_mlkem768_invalid_pk_length() {
        let kem = MlKem768Kem::new();
        // 使用错误长度公钥应返回错误
        let result = kem.encapsulate(&[0u8; 100]);
        assert!(
            result.is_err(),
            "encapsulate with invalid pk length must fail"
        );
    }

    #[test]
    fn test_hybrid_exchange_with_real_mlkem() {
        // 使用真实 ML-KEM-768 作为后量子轨道
        let ex = HybridKeyExchange::new(Box::new(MockKem::new()), Box::new(MlKem768Kem::new()));
        let recipient = ex.generate_keypair().unwrap();
        let enc = ex.encapsulate(&recipient).unwrap();
        let shared = ex.decapsulate(&recipient, &enc).unwrap();

        assert_eq!(shared.len(), HYBRID_KEY_LEN);
        assert_eq!(
            shared, enc.final_key,
            "hybrid key must match after decapsulation"
        );
        assert_eq!(ex.classical_algorithm(), KemAlgorithm::MockX25519);
        assert_eq!(ex.post_quantum_algorithm(), KemAlgorithm::MlKem768);
    }
}
