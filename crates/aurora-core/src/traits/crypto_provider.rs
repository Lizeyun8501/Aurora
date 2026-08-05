//! CryptoProvider Trait — 密码学能力抽象（V19 §28.6）
//!
//! 对应架构设计报告 V19 七大 Trait 之一。V19 规定 `CryptoEngine` 为**跨层服务**，
//! 所有模块必须通过 `CryptoProvider` Trait 访问密码学能力，禁止直接依赖
//! ring / aes-gcm / argon2 等具体实现 crate。
//!
//! 覆盖能力：
//! - AES-256-GCM 对称加解密
//! - Argon2id 密钥派生
//! - ML-KEM-768 后量子密钥封装（与 X25519 双轨并行，见 aurora-security）
//! - Ed25519 签名验证（插件市场签名校验）
//! - 安全随机数、SHA-256、HMAC-SHA256
//! - 算法版本号（前向兼容：密文格式升级时可识别旧版本）
//!
//! # 异步化设计决策
//! V19 §28 原始指定 `async_trait`，但 CryptoProvider 的所有方法均为 CPU 密集型
//! 纯计算（无 IO），async 化会引入不必要的 `Pin<Box<Future>>` 堆分配开销。
//! 经架构审阅决定：**CryptoProvider 保持同步签名**，与其余 10 个 Trait 的 async
//! 签名并存。未来如需接入远程 HSM 或 KMS 服务，可派生 `AsyncCryptoProvider` 子 Trait。

use serde::{Deserialize, Serialize};

/// AES-256-GCM 密文结构（nonce + data + tag 分离存储）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ciphertext {
    /// 96-bit GCM nonce。
    pub nonce: [u8; 12],
    /// 密文主体。
    pub data: Vec<u8>,
    /// 128-bit GCM 认证标签。
    pub tag: [u8; 16],
}

/// KEM 公钥（ML-KEM-768，1184 字节）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KemPublicKey(pub Vec<u8>);

/// KEM 私钥（ML-KEM-768，2400 字节）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KemSecretKey(pub Vec<u8>);

/// KEM 封装产生的共享密钥（32 字节）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KemSharedSecret(pub [u8; 32]);

/// KEM 封装密文（ML-KEM-768，1088 字节）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KemCiphertext(pub Vec<u8>);

/// Ed25519 公钥（32 字节）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ed25519PublicKey(pub [u8; 32]);

/// Ed25519 签名（64 字节）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ed25519Signature(pub [u8; 64]);

/// 密码学能力抽象接口（跨层服务）。
pub trait CryptoProvider: Send + Sync {
    /// AES-256-GCM 加密。
    fn encrypt(&self, plaintext: &[u8], key: &[u8; 32]) -> Result<Ciphertext, crate::Error>;

    /// AES-256-GCM 解密（认证失败返回 [`crate::Error::Crypto`]）。
    fn decrypt(&self, ciphertext: &Ciphertext, key: &[u8; 32]) -> Result<Vec<u8>, crate::Error>;

    /// Argon2id 密钥派生（V19 参数：64MB 内存 / 3 次迭代 / 4 并行）。
    fn derive_key(&self, password: &str, salt: &[u8]) -> Result<[u8; 32], crate::Error>;

    /// 生成 ML-KEM-768 密钥对。
    fn kem_keypair(&self) -> Result<(KemPublicKey, KemSecretKey), crate::Error>;

    /// KEM 封装：用接收方公钥生成共享密钥与封装密文。
    fn kem_encapsulate(
        &self,
        pk: &KemPublicKey,
    ) -> Result<(KemSharedSecret, KemCiphertext), crate::Error>;

    /// KEM 解封装：用本地私钥从封装密文恢复共享密钥。
    fn kem_decapsulate(
        &self,
        sk: &KemSecretKey,
        ct: &KemCiphertext,
    ) -> Result<KemSharedSecret, crate::Error>;

    /// 生成密码学安全随机字节。
    fn random_bytes(&self, len: usize) -> Vec<u8>;

    /// SHA-256 哈希。
    fn hash(&self, data: &[u8]) -> [u8; 32];

    /// HMAC-SHA256 签名。
    fn hmac_sign(&self, key: &[u8], data: &[u8]) -> Vec<u8>;

    /// HMAC-SHA256 验证（常数时间比较）。
    fn hmac_verify(&self, key: &[u8], data: &[u8], signature: &[u8]) -> bool;

    /// Ed25519 签名验证（插件市场签名校验等场景）。
    ///
    /// 返回 `true` 表示签名有效，`false` 表示签名不匹配或格式错误。
    fn ed25519_verify(
        &self,
        public_key: &Ed25519PublicKey,
        message: &[u8],
        signature: &Ed25519Signature,
    ) -> bool;

    /// 算法版本号（用于密文前向兼容与迁移）。
    ///
    /// 当前版本：`1` = AES-256-GCM + Argon2id + ML-KEM-768。
    fn algorithm_version(&self) -> u16;
}
