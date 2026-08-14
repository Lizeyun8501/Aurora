//! 密钥层次结构 (Key Hierarchy)
//!
//! 用户口令 → Argon2id 派生主密钥 (Master Key)，
//! 每个 Workspace 拥有独立的数据加密密钥 (DEK, Data Encryption Key)。
//!
//! # 设计要点
//! - Argon2id 参数：内存 64MB、迭代 3 次、并行度 4（参见 `ARGON2_*` 常量）。
//! - 主密钥仅在内存中存在，落盘时只存储盐值与 DEK 密文。
//! - DEK 为 256-bit 随机密钥，用于 AES-256-GCM 内容加密。

use std::collections::HashMap;
use std::sync::Arc;

use argon2::{Algorithm, Argon2, Params, Version};
use parking_lot::RwLock;
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use tracing::{debug, info};
use uuid::Uuid;

use crate::Error;

/// 主密钥长度（256-bit / 32 字节）
pub const MASTER_KEY_LEN: usize = 32;
/// DEK 长度（256-bit AES 密钥）
pub const DEK_LEN: usize = 32;
/// Argon2id 盐长度
pub const SALT_LEN: usize = 16;

/// Argon2id 内存开销：64 MB（单位 KiB）
pub const ARGON2_M_COST: u32 = 64 * 1024;
/// Argon2id 迭代次数
pub const ARGON2_T_COST: u32 = 3;
/// Argon2id 并行度
pub const ARGON2_P_COST: u32 = 4;

/// 主密钥：由用户口令经 Argon2id 派生
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MasterKey {
    /// 派生后的 256-bit 主密钥材料
    key: [u8; MASTER_KEY_LEN],
    /// Argon2id 盐值
    salt: [u8; SALT_LEN],
}

impl MasterKey {
    /// 从用户口令派生主密钥（自动生成盐值）
    pub fn derive(password: &str) -> Result<Self, Error> {
        let mut salt = [0u8; SALT_LEN];
        OsRng.fill_bytes(&mut salt);
        Self::derive_with_salt(password, &salt)
    }

    /// 使用指定盐值派生主密钥（用于解锁已存在的保险库）
    pub fn derive_with_salt(password: &str, salt: &[u8]) -> Result<Self, Error> {
        if salt.len() < SALT_LEN {
            return Err(Error::InvalidInput(format!(
                "salt too short: {} < {}",
                salt.len(),
                SALT_LEN
            )));
        }
        let mut salt_arr = [0u8; SALT_LEN];
        salt_arr.copy_from_slice(&salt[..SALT_LEN]);

        let params = Params::new(
            ARGON2_M_COST,
            ARGON2_T_COST,
            ARGON2_P_COST,
            Some(MASTER_KEY_LEN),
        )
        .map_err(|e| Error::Kdf(format!("argon2 params error: {}", e)))?;
        let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

        let mut key = [0u8; MASTER_KEY_LEN];
        argon2
            .hash_password_into(password.as_bytes(), &salt_arr, &mut key)
            .map_err(|e| Error::Kdf(format!("argon2 hash error: {}", e)))?;

        debug!("derived 256-bit master key via Argon2id");
        Ok(Self {
            key,
            salt: salt_arr,
        })
    }

    /// 返回主密钥字节切片
    pub fn as_bytes(&self) -> &[u8] {
        &self.key
    }

    /// 返回盐值
    pub fn salt(&self) -> &[u8; SALT_LEN] {
        &self.salt
    }

    /// 安全擦除密钥材料
    fn zeroize(&mut self) {
        // 使用 volatile 写入擦除密钥材料，防止编译器优化
        for byte in self.key.iter_mut() {
            unsafe { std::ptr::write_volatile(byte, 0) };
        }
        for byte in self.salt.iter_mut() {
            unsafe { std::ptr::write_volatile(byte, 0) };
        }
    }
}

impl Drop for MasterKey {
    fn drop(&mut self) {
        self.zeroize();
    }
}

/// 工作区数据加密密钥 (DEK)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceDek {
    /// 所属工作区 ID
    pub workspace_id: String,
    /// 256-bit DEK
    key: [u8; DEK_LEN],
    /// 创建时间
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl WorkspaceDek {
    /// 为指定工作区生成随机 DEK
    pub fn new(workspace_id: impl Into<String>) -> Result<Self, Error> {
        let mut key = [0u8; DEK_LEN];
        OsRng.fill_bytes(&mut key);
        Ok(Self {
            workspace_id: workspace_id.into(),
            key,
            created_at: chrono::Utc::now(),
        })
    }

    /// 从已有字节数组构造 DEK
    pub fn from_bytes(workspace_id: impl Into<String>, key: [u8; DEK_LEN]) -> Self {
        Self {
            workspace_id: workspace_id.into(),
            key,
            created_at: chrono::Utc::now(),
        }
    }

    /// 返回 DEK 字节切片
    pub fn as_bytes(&self) -> &[u8] {
        &self.key
    }

    /// 安全擦除密钥材料
    fn zeroize(&mut self) {
        for byte in self.key.iter_mut() {
            unsafe { std::ptr::write_volatile(byte, 0) };
        }
    }

    /// 返回 DEK 长度
    pub fn len(&self) -> usize {
        DEK_LEN
    }

    /// 是否为空（DEK 恒为 32 字节，始终返回 false）
    pub fn is_empty(&self) -> bool {
        false
    }
}

impl Drop for WorkspaceDek {
    fn drop(&mut self) {
        self.zeroize();
    }
}

/// 密钥层次结构：管理主密钥与多个工作区 DEK
pub struct KeyHierarchy {
    master: Arc<RwLock<Option<MasterKey>>>,
    deks: Arc<RwLock<HashMap<String, WorkspaceDek>>>,
}

impl Default for KeyHierarchy {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyHierarchy {
    pub fn new() -> Self {
        Self {
            master: Arc::new(RwLock::new(None)),
            deks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 使用口令初始化/解锁主密钥（自动生成新盐值）
    pub fn unlock(&self, password: &str) -> Result<MasterKey, Error> {
        let mk = MasterKey::derive(password)?;
        *self.master.write() = Some(mk.clone());
        info!("key hierarchy unlocked (new salt)");
        Ok(mk)
    }

    /// 使用已有盐值解锁主密钥
    pub fn unlock_with_salt(&self, password: &str, salt: &[u8]) -> Result<MasterKey, Error> {
        let mk = MasterKey::derive_with_salt(password, salt)?;
        *self.master.write() = Some(mk.clone());
        info!("key hierarchy unlocked (existing salt)");
        Ok(mk)
    }

    /// 锁定：清除内存中的主密钥
    pub fn lock(&self) {
        *self.master.write() = None;
        info!("key hierarchy locked");
    }

    /// 主密钥是否就绪
    pub fn is_unlocked(&self) -> bool {
        self.master.read().is_some()
    }

    /// 获取主密钥副本
    pub fn master(&self) -> Option<MasterKey> {
        self.master.read().clone()
    }

    /// 为工作区创建并注册新 DEK
    pub fn create_dek(&self, workspace_id: impl Into<String>) -> Result<WorkspaceDek, Error> {
        let ws = workspace_id.into();
        let dek = WorkspaceDek::new(&ws)?;
        self.deks.write().insert(ws, dek.clone());
        debug!("created DEK for workspace");
        Ok(dek)
    }

    /// 注册已有 DEK
    pub fn register_dek(&self, dek: WorkspaceDek) {
        self.deks.write().insert(dek.workspace_id.clone(), dek);
    }

    /// 获取工作区 DEK
    pub fn get_dek(&self, workspace_id: &str) -> Option<WorkspaceDek> {
        self.deks.read().get(workspace_id).cloned()
    }

    /// 移除工作区 DEK
    pub fn remove_dek(&self, workspace_id: &str) -> Option<WorkspaceDek> {
        self.deks.write().remove(workspace_id)
    }

    /// 列出全部 DEK
    pub fn list_deks(&self) -> Vec<WorkspaceDek> {
        self.deks.read().values().cloned().collect()
    }

    /// 已注册工作区数量
    pub fn dek_count(&self) -> usize {
        self.deks.read().len()
    }
}

/// 生成一个新的工作区 ID（UUID v4）
pub fn new_workspace_id() -> String {
    Uuid::new_v4().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_master_key_derive_deterministic_with_salt() {
        let salt = [0xABu8; SALT_LEN];
        let mk1 = MasterKey::derive_with_salt("correct horse battery", &salt).unwrap();
        let mk2 = MasterKey::derive_with_salt("correct horse battery", &salt).unwrap();
        assert_eq!(
            mk1.as_bytes(),
            mk2.as_bytes(),
            "same password+salt must derive same key"
        );
        assert_eq!(mk1.salt(), &salt);
    }

    #[test]
    fn test_master_key_different_passwords_differ() {
        let salt = [0x11u8; SALT_LEN];
        let mk1 = MasterKey::derive_with_salt("password-a", &salt).unwrap();
        let mk2 = MasterKey::derive_with_salt("password-b", &salt).unwrap();
        assert_ne!(mk1.as_bytes(), mk2.as_bytes());
    }

    #[test]
    fn test_master_key_different_salts_differ() {
        let mk1 = MasterKey::derive_with_salt("same-pass", &[0u8; SALT_LEN]).unwrap();
        let mk2 = MasterKey::derive_with_salt("same-pass", &[1u8; SALT_LEN]).unwrap();
        assert_ne!(mk1.as_bytes(), mk2.as_bytes());
    }

    #[test]
    fn test_master_key_derive_auto_salt() {
        let mk = MasterKey::derive("hunter2").unwrap();
        assert_eq!(mk.as_bytes().len(), MASTER_KEY_LEN);
        // 验证盐值可复用
        let mk2 = MasterKey::derive_with_salt("hunter2", mk.salt()).unwrap();
        assert_eq!(mk.as_bytes(), mk2.as_bytes());
    }

    #[test]
    fn test_master_key_short_salt_rejected() {
        let result = MasterKey::derive_with_salt("pw", &[0u8; 8]);
        assert!(result.is_err());
    }

    #[test]
    fn test_dek_generation_uniqueness() {
        let d1 = WorkspaceDek::new("ws-1").unwrap();
        let d2 = WorkspaceDek::new("ws-2").unwrap();
        assert_eq!(d1.len(), DEK_LEN);
        assert_ne!(d1.as_bytes(), d2.as_bytes());
        assert!(!d1.is_empty());
    }

    #[test]
    fn test_dek_from_bytes() {
        let key = [0x42u8; DEK_LEN];
        let dek = WorkspaceDek::from_bytes("ws-x", key);
        assert_eq!(dek.as_bytes(), &key);
        assert_eq!(dek.workspace_id, "ws-x");
    }

    #[test]
    fn test_key_hierarchy_unlock_lock() {
        let kh = KeyHierarchy::new();
        assert!(!kh.is_unlocked());

        let mk = kh.unlock("my-password").unwrap();
        assert!(kh.is_unlocked());
        assert_eq!(kh.master().unwrap().as_bytes(), mk.as_bytes());

        kh.lock();
        assert!(!kh.is_unlocked());
        assert!(kh.master().is_none());
    }

    #[test]
    fn test_key_hierarchy_unlock_with_salt_roundtrip() {
        let kh = KeyHierarchy::new();
        let mk = kh.unlock("secret").unwrap();
        let salt = *mk.salt();

        kh.lock();
        let mk2 = kh.unlock_with_salt("secret", &salt).unwrap();
        assert_eq!(mk.as_bytes(), mk2.as_bytes());

        // 错误口令派生出不同主密钥
        kh.lock();
        let mk_wrong = kh.unlock_with_salt("wrong", &salt).unwrap();
        assert_ne!(mk.as_bytes(), mk_wrong.as_bytes());
    }

    #[test]
    fn test_key_hierarchy_dek_management() {
        let kh = KeyHierarchy::new();
        let ws_id = new_workspace_id();

        let dek = kh.create_dek(&ws_id).unwrap();
        assert_eq!(kh.dek_count(), 1);
        assert!(kh.get_dek(&ws_id).is_some());

        let fetched = kh.get_dek(&ws_id).unwrap();
        assert_eq!(fetched.as_bytes(), dek.as_bytes());

        let removed = kh.remove_dek(&ws_id).unwrap();
        assert_eq!(removed.workspace_id, ws_id);
        assert_eq!(kh.dek_count(), 0);
        assert!(kh.get_dek(&ws_id).is_none());
    }

    #[test]
    fn test_key_hierarchy_register_and_list_deks() {
        let kh = KeyHierarchy::new();
        let d1 = WorkspaceDek::new("ws-a").unwrap();
        let d2 = WorkspaceDek::new("ws-b").unwrap();
        kh.register_dek(d1);
        kh.register_dek(d2);
        assert_eq!(kh.dek_count(), 2);
        assert_eq!(kh.list_deks().len(), 2);
    }
}
