//! 多设备管理 (Multi-Device Management)
//!
//! # 设备身份
//! 每台设备拥有 Ed25519 密钥对，公钥派生 [`DeviceId`]。
//! 新设备通过 QR 码扫描 ([`QrAuthorization`]) 授权加入，
//! 所有者可远程吊销设备并使该设备的 DEK 失效。
//!
//! # DEK 失效
//! 设备吊销后，其缓存的 DEK (Data Encryption Key) 通过递增全局 DEK 版本号
//! 触发密钥重包装，被吊销设备无法解密后续同步数据。
//!
//! # 实现说明
//! [`DeviceId::random`] 模拟 Ed25519 公钥的 hex 编码 (真实实现使用 `ring::signature::Ed25519KeyPair`)。

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

/// 设备 ID (Ed25519 公钥的 hex 编码)。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DeviceId(String);

impl DeviceId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// 模拟 Ed25519 公钥派生 (真实实现使用 ring::signature::Ed25519KeyPair)。
    pub fn random() -> Self {
        // 64 字符 hex (32 字节)
        let uuid = uuid::Uuid::new_v4();
        let bytes = uuid.as_bytes();
        let hex: String = bytes
            .iter()
            .chain(bytes.iter())
            .map(|b| format!("{:02x}", b))
            .collect();
        Self(hex)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for DeviceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// 设备状态。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum DeviceStatus {
    /// 已激活。
    Active,
    /// 待授权 (等待 QR 扫描确认)。
    Pending,
    /// 已吊销。
    Revoked,
}

/// 设备记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub device_id: DeviceId,
    pub display_name: String,
    pub status: DeviceStatus,
    pub public_key: Vec<u8>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_seen: Option<chrono::DateTime<chrono::Utc>>,
    /// 当前有效的 DEK 包装版本号 (吊销时递增使旧包装失效)。
    pub dek_version: u32,
}

impl Device {
    pub fn new(device_id: DeviceId, display_name: impl Into<String>, public_key: Vec<u8>) -> Self {
        Self {
            device_id,
            display_name: display_name.into(),
            status: DeviceStatus::Pending,
            public_key,
            created_at: chrono::Utc::now(),
            last_seen: None,
            dek_version: 1,
        }
    }

    pub fn is_active(&self) -> bool {
        self.status == DeviceStatus::Active
    }
}

/// QR 授权令牌。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QrAuthorization {
    pub token: String,
    pub requesting_device_id: DeviceId,
    pub authorizing_device_id: DeviceId,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub used: bool,
}

impl QrAuthorization {
    /// 创建授权令牌，TTL 由 `ttl_seconds` 指定。
    pub fn new(requesting: DeviceId, authorizing: DeviceId, ttl_seconds: i64) -> Self {
        let now = chrono::Utc::now();
        let expires = now + chrono::Duration::seconds(ttl_seconds);
        Self {
            token: uuid::Uuid::new_v4().to_string(),
            requesting_device_id: requesting,
            authorizing_device_id: authorizing,
            created_at: now,
            expires_at: expires,
            used: false,
        }
    }

    pub fn is_expired(&self) -> bool {
        chrono::Utc::now() > self.expires_at
    }

    pub fn is_valid(&self) -> bool {
        !self.used && !self.is_expired()
    }
}

/// 设备管理器。
pub struct DeviceManager {
    devices: Arc<RwLock<HashMap<DeviceId, Device>>>,
    pending_auths: Arc<RwLock<HashMap<String, QrAuthorization>>>,
    /// 全局 DEK 版本号 (每次吊销递增)。
    global_dek_version: Arc<RwLock<u32>>,
}

impl DeviceManager {
    pub fn new() -> Self {
        Self {
            devices: Arc::new(RwLock::new(HashMap::new())),
            pending_auths: Arc::new(RwLock::new(HashMap::new())),
            global_dek_version: Arc::new(RwLock::new(1)),
        }
    }

    /// 注册所有者设备 (首个设备，自动激活)。
    ///
    /// 第一台设备无需 QR 授权，直接成为所有者。
    pub fn register_owner(&self, mut device: Device) -> DeviceId {
        let id = device.device_id.clone();
        device.status = DeviceStatus::Active;
        device.dek_version = *self.global_dek_version.read();
        debug!("owner device registered: {} ({})", id, device.display_name);
        self.devices.write().insert(id.clone(), device);
        id
    }

    /// 注册新设备 (Pending 状态，等待 QR 授权)。
    pub fn register_device(&self, device: Device) -> DeviceId {
        let id = device.device_id.clone();
        debug!("device registered: {} ({})", id, device.display_name);
        self.devices.write().insert(id.clone(), device);
        id
    }

    /// 发起 QR 授权：由已激活设备为待授权设备生成令牌。
    pub fn initiate_qr_auth(
        &self,
        requesting: &DeviceId,
        authorizing: &DeviceId,
    ) -> crate::Result<QrAuthorization> {
        {
            let devices = self.devices.read();
            let auth_device = devices.get(authorizing).ok_or_else(|| {
                crate::Error::Device(format!("authorizing device not found: {}", authorizing))
            })?;
            if !auth_device.is_active() {
                return Err(crate::Error::Unauthorized(format!(
                    "authorizing device not active: {}",
                    authorizing
                )));
            }
            if !devices.contains_key(requesting) {
                return Err(crate::Error::Device(format!(
                    "requesting device not found: {}",
                    requesting
                )));
            }
        }
        let auth = QrAuthorization::new(requesting.clone(), authorizing.clone(), 300);
        let token = auth.token.clone();
        self.pending_auths.write().insert(token, auth.clone());
        info!("qr auth initiated: {} -> {}", authorizing, requesting);
        Ok(auth)
    }

    /// 完成 QR 授权：消费令牌，激活待授权设备。
    pub fn complete_qr_auth(&self, token: &str) -> crate::Result<Device> {
        let requesting = {
            let mut auths = self.pending_auths.write();
            let auth = auths.get_mut(token).ok_or_else(|| {
                crate::Error::Unauthorized(format!("invalid auth token: {}", token))
            })?;
            if !auth.is_valid() {
                return Err(crate::Error::Unauthorized(format!(
                    "auth token expired or used: {}",
                    token
                )));
            }
            auth.used = true;
            auth.requesting_device_id.clone()
        };
        let current_dek_version = *self.global_dek_version.read();
        let mut devices = self.devices.write();
        let device = devices
            .get_mut(&requesting)
            .ok_or_else(|| crate::Error::Device(format!("device not found: {}", requesting)))?;
        device.status = DeviceStatus::Active;
        device.dek_version = current_dek_version;
        device.last_seen = Some(chrono::Utc::now());
        info!("device activated: {}", requesting);
        Ok(device.clone())
    }

    /// 远程吊销设备：标记 Revoked 并递增全局 DEK 版本，
    /// 使被吊销设备缓存的 DEK 包装失效。
    pub fn revoke_device(&self, device_id: &DeviceId) -> crate::Result<u32> {
        {
            let mut devices = self.devices.write();
            let device = devices
                .get_mut(device_id)
                .ok_or_else(|| crate::Error::Device(format!("device not found: {}", device_id)))?;
            if device.status == DeviceStatus::Revoked {
                warn!("device already revoked: {}", device_id);
                return Ok(*self.global_dek_version.read());
            }
            device.status = DeviceStatus::Revoked;
        }
        // 递增全局 DEK 版本，触发所有仍激活设备的密钥重包装
        let new_version = {
            let mut gv = self.global_dek_version.write();
            *gv += 1;
            *gv
        };
        let mut devices = self.devices.write();
        for dev in devices.values_mut() {
            if dev.status == DeviceStatus::Active {
                dev.dek_version = new_version;
            }
        }
        info!(
            "device revoked: {} new_dek_version={}",
            device_id, new_version
        );
        Ok(new_version)
    }

    /// 列出全部设备。
    pub fn list_devices(&self) -> Vec<Device> {
        self.devices.read().values().cloned().collect()
    }

    /// 列出激活设备。
    pub fn active_devices(&self) -> Vec<Device> {
        self.devices
            .read()
            .values()
            .filter(|d| d.status == DeviceStatus::Active)
            .cloned()
            .collect()
    }

    /// 检查某设备是否仍可解密当前 DEK 版本。
    pub fn can_decrypt(&self, device_id: &DeviceId) -> bool {
        let devices = self.devices.read();
        let device = match devices.get(device_id) {
            Some(d) => d,
            None => return false,
        };
        if !device.is_active() {
            return false;
        }
        device.dek_version == *self.global_dek_version.read()
    }

    /// 当前全局 DEK 版本。
    pub fn global_dek_version(&self) -> u32 {
        *self.global_dek_version.read()
    }

    /// 设备总数。
    pub fn device_count(&self) -> usize {
        self.devices.read().len()
    }
}

impl Default for DeviceManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_device(name: &str) -> Device {
        Device::new(DeviceId::random(), name, vec![0u8; 32])
    }

    #[test]
    fn test_device_id_random_unique() {
        let a = DeviceId::random();
        let b = DeviceId::random();
        assert_ne!(a, b);
        assert_eq!(a.as_str().len(), 64); // 32 字节 hex
    }

    #[test]
    fn test_device_new_is_pending() {
        let dev = make_device("Laptop");
        assert_eq!(dev.status, DeviceStatus::Pending);
        assert!(!dev.is_active());
        assert_eq!(dev.display_name, "Laptop");
    }

    #[test]
    fn test_qr_authorization_validity() {
        let req = DeviceId::new("req");
        let auth = DeviceId::new("auth");
        let token = QrAuthorization::new(req.clone(), auth.clone(), 300);
        assert!(token.is_valid());
        assert!(!token.is_expired());
        // 过期令牌
        let expired = QrAuthorization::new(req, auth, -1);
        assert!(!expired.is_valid());
    }

    #[test]
    fn test_register_owner_auto_active() {
        let mgr = DeviceManager::new();
        let owner = make_device("Owner-Phone");
        let id = mgr.register_owner(owner);
        assert_eq!(mgr.device_count(), 1);
        assert_eq!(mgr.active_devices().len(), 1);
        assert!(mgr.can_decrypt(&id));
        assert_eq!(mgr.global_dek_version(), 1);
    }

    #[test]
    fn test_qr_auth_complete_flow() {
        let mgr = DeviceManager::new();
        let owner = make_device("Owner-Phone");
        let owner_id = mgr.register_owner(owner);
        let laptop = make_device("Laptop");
        let laptop_id = mgr.register_device(laptop);
        // laptop 初始 Pending，不能解密
        assert!(!mgr.can_decrypt(&laptop_id));
        // owner 为 laptop 发起授权
        let auth = mgr
            .initiate_qr_auth(&laptop_id, &owner_id)
            .expect("initiate");
        // 完成授权
        let activated = mgr.complete_qr_auth(&auth.token).expect("complete");
        assert_eq!(activated.status, DeviceStatus::Active);
        assert!(mgr.can_decrypt(&laptop_id));
    }

    #[test]
    fn test_initiate_qr_auth_requires_active_authorizer() {
        let mgr = DeviceManager::new();
        let d1 = make_device("d1");
        let d2 = make_device("d2");
        let id1 = mgr.register_device(d1); // Pending
        let id2 = mgr.register_device(d2); // Pending
                                           // 两个都 Pending，不能互相授权
        let result = mgr.initiate_qr_auth(&id2, &id1);
        assert!(result.is_err());
    }

    #[test]
    fn test_revoke_device_increments_dek() {
        let mgr = DeviceManager::new();
        let owner = make_device("Owner");
        let owner_id = mgr.register_owner(owner);
        assert_eq!(mgr.global_dek_version(), 1);
        let new_version = mgr.revoke_device(&owner_id).expect("revoke");
        assert_eq!(new_version, 2);
        assert_eq!(mgr.global_dek_version(), 2);
    }

    #[test]
    fn test_revoked_device_cannot_decrypt() {
        let mgr = DeviceManager::new();
        let owner = make_device("Owner");
        let owner_id = mgr.register_owner(owner);
        // 吊销前可解密
        assert!(mgr.can_decrypt(&owner_id));
        mgr.revoke_device(&owner_id).unwrap();
        // 吊销后不可解密
        assert!(!mgr.can_decrypt(&owner_id));
    }

    #[test]
    fn test_revoke_one_device_updates_others_dek() {
        let mgr = DeviceManager::new();
        let owner = make_device("Owner");
        let owner_id = mgr.register_owner(owner);
        let laptop = make_device("Laptop");
        let laptop_id = mgr.register_device(laptop);
        let auth = mgr.initiate_qr_auth(&laptop_id, &owner_id).unwrap();
        mgr.complete_qr_auth(&auth.token).unwrap();
        // 两者都激活，DEK 版本 = 1
        assert!(mgr.can_decrypt(&laptop_id));
        assert_eq!(mgr.global_dek_version(), 1);
        // 吊销 laptop
        let new_version = mgr.revoke_device(&laptop_id).unwrap();
        assert_eq!(new_version, 2);
        // owner 仍可解密 (DEK 版本被更新到 2)
        assert!(mgr.can_decrypt(&owner_id));
        // laptop 不可解密
        assert!(!mgr.can_decrypt(&laptop_id));
    }

    #[test]
    fn test_complete_qr_auth_invalid_token() {
        let mgr = DeviceManager::new();
        let result = mgr.complete_qr_auth("nonexistent-token");
        assert!(result.is_err());
    }

    #[test]
    fn test_revoke_unknown_device_errors() {
        let mgr = DeviceManager::new();
        let ghost = DeviceId::new("ghost");
        let result = mgr.revoke_device(&ghost);
        assert!(result.is_err());
    }
}
