//! 密码学 (ring / aes-gcm / argon2 / sha3)
//!
//! 提供端到端加密、密钥派生、哈希与签名等密码学能力，保障数据安全。
//!
//! - [ring](https://briansmith.org/rustdoc/ring/)：底层密码学原语
//! - [aes-gcm](https://docs.rs/aes-gcm)：AES-GCM 对称加密
//! - [argon2](https://docs.rs/argon2)：Argon2 密钥派生 / 口令哈希
//! - [sha3](https://docs.rs/sha3)：SHA-3 哈希族

/// 密码学上下文占位类型。
///
/// 实际实现将在后续任务中封装密钥管理与加解密能力。
pub struct CryptoContext;
