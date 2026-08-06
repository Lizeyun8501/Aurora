//! 生物识别保护 (Biometric Protection)
//!
//! 移动端 FaceID/TouchID、桌面端 TPM/Secure Enclave 的统一抽象，
//! 支持有效期配置（如 5 分钟重新提示）。
//!
//! # 设计
//! - [`BiometricAuthenticator`] trait 抽象平台生物识别能力。
//! - [`MockBiometricAuthenticator`] 提供可测试的内存实现。
//! - [`BiometricProtector`] 在敏感操作前强制校验生物识别状态。

use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::Error;

/// 默认有效期：5 分钟（300 秒）
pub const DEFAULT_VALIDITY_SECS: u64 = 300;

/// 生物识别类型
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum BiometricKind {
    /// 移动端 Face ID
    FaceId,
    /// 移动端 Touch ID / 指纹
    TouchId,
    /// 桌面端 TPM
    Tpm,
    /// 桌面端 Secure Enclave
    SecureEnclave,
    /// 未启用 / 不可用
    None,
}

impl BiometricKind {
    pub fn is_available(&self) -> bool {
        !matches!(self, BiometricKind::None)
    }
}

/// 生物识别状态
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[derive(Default)]
pub enum BiometricStatus {
    /// 未认证
    #[default]
    Unauthenticated,
    /// 已认证：记录认证时刻与方式
    Authenticated {
        at: chrono::DateTime<chrono::Utc>,
        kind: BiometricKind,
    },
    /// 硬件不可用 / 被禁用
    Unavailable,
    /// 认证失败
    Failed,
}

impl BiometricStatus {
    pub fn is_authenticated(&self) -> bool {
        matches!(self, BiometricStatus::Authenticated { .. })
    }
}


/// 生物识别配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiometricConfig {
    /// 认证有效期（秒），超过需重新认证
    pub validity_secs: u64,
    /// 启用的生物识别类型
    pub kind: BiometricKind,
    /// 认证不可用时是否允许回退到口令
    pub allow_password_fallback: bool,
}

impl Default for BiometricConfig {
    fn default() -> Self {
        Self {
            validity_secs: DEFAULT_VALIDITY_SECS,
            kind: BiometricKind::None,
            allow_password_fallback: true,
        }
    }
}

impl BiometricConfig {
    pub fn new(kind: BiometricKind) -> Self {
        Self {
            validity_secs: DEFAULT_VALIDITY_SECS,
            kind,
            allow_password_fallback: true,
        }
    }

    /// 设置有效期（秒）
    pub fn with_validity_secs(mut self, secs: u64) -> Self {
        self.validity_secs = secs;
        self
    }

    /// 设置是否允许口令回退
    pub fn with_password_fallback(mut self, allow: bool) -> Self {
        self.allow_password_fallback = allow;
        self
    }
}

/// 生物识别认证器 trait
pub trait BiometricAuthenticator: Send + Sync {
    /// 认证方式
    fn kind(&self) -> BiometricKind;
    /// 硬件是否可用
    fn is_available(&self) -> bool;
    /// 触发一次认证
    fn authenticate(&self) -> Result<BiometricStatus, Error>;
    /// 当前状态
    fn current_status(&self) -> BiometricStatus;
    /// 使当前认证失效
    fn invalidate(&self);
    /// 当前是否仍处于有效期内（基于系统时钟）
    fn is_authenticated(&self) -> bool;
    /// 当前是否仍处于有效期内（基于给定时间，便于测试）
    fn is_authenticated_at(&self, now: chrono::DateTime<chrono::Utc>) -> bool;
}

/// Mock 生物识别认证器
pub struct MockBiometricAuthenticator {
    config: BiometricConfig,
    status: Arc<RwLock<BiometricStatus>>,
    /// 控制下一次 `authenticate` 是否成功
    next_success: Arc<RwLock<bool>>,
    /// 控制硬件是否可用
    available: Arc<RwLock<bool>>,
}

impl MockBiometricAuthenticator {
    pub fn new(config: BiometricConfig) -> Self {
        Self {
            config,
            status: Arc::new(RwLock::new(BiometricStatus::Unauthenticated)),
            next_success: Arc::new(RwLock::new(true)),
            available: Arc::new(RwLock::new(true)),
        }
    }

    /// 配置下一次 `authenticate` 的成功/失败
    pub fn set_next_success(&self, success: bool) {
        *self.next_success.write() = success;
    }

    /// 设置硬件可用性
    pub fn set_available(&self, available: bool) {
        *self.available.write() = available;
    }

    /// 使用指定时刻触发认证（便于测试）
    pub fn authenticate_at(
        &self,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<BiometricStatus, Error> {
        if !*self.available.read() {
            let status = BiometricStatus::Unavailable;
            *self.status.write() = status;
            warn!("biometric hardware unavailable");
            return Ok(status);
        }
        let success = *self.next_success.read();
        let status = if success {
            BiometricStatus::Authenticated {
                at: now,
                kind: self.config.kind,
            }
        } else {
            BiometricStatus::Failed
        };
        *self.status.write() = status;
        debug!(success, "biometric authenticate");
        Ok(status)
    }

    pub fn config(&self) -> &BiometricConfig {
        &self.config
    }
}

impl Default for MockBiometricAuthenticator {
    fn default() -> Self {
        Self::new(BiometricConfig::default())
    }
}

impl BiometricAuthenticator for MockBiometricAuthenticator {
    fn kind(&self) -> BiometricKind {
        self.config.kind
    }

    fn is_available(&self) -> bool {
        *self.available.read()
    }

    fn authenticate(&self) -> Result<BiometricStatus, Error> {
        self.authenticate_at(chrono::Utc::now())
    }

    fn current_status(&self) -> BiometricStatus {
        *self.status.read()
    }

    fn invalidate(&self) {
        *self.status.write() = BiometricStatus::Unauthenticated;
        info!("biometric authentication invalidated");
    }

    fn is_authenticated(&self) -> bool {
        self.is_authenticated_at(chrono::Utc::now())
    }

    fn is_authenticated_at(&self, now: chrono::DateTime<chrono::Utc>) -> bool {
        match *self.status.read() {
            BiometricStatus::Authenticated { at, .. } => {
                let elapsed = now.signed_duration_since(at);
                elapsed.num_seconds() >= 0
                    && (elapsed.num_seconds() as u64) <= self.config.validity_secs
            }
            _ => false,
        }
    }
}

/// 生物识别保护器：在敏感操作前强制校验
pub struct BiometricProtector {
    authenticator: Box<dyn BiometricAuthenticator>,
    config: BiometricConfig,
}

impl BiometricProtector {
    pub fn new(authenticator: Box<dyn BiometricAuthenticator>, config: BiometricConfig) -> Self {
        Self {
            authenticator,
            config,
        }
    }

    /// 确保当前已通过生物识别（仍在有效期内则直接放行）
    pub fn ensure_authenticated(&self) -> Result<(), Error> {
        if self.authenticator.is_authenticated() {
            return Ok(());
        }
        let status = self.authenticator.authenticate()?;
        match status {
            BiometricStatus::Authenticated { .. } => Ok(()),
            BiometricStatus::Unavailable if self.config.allow_password_fallback => Ok(()),
            other => Err(Error::Biometric(format!(
                "authentication required (status={:?})",
                other
            ))),
        }
    }

    /// 仅检查不触发认证
    pub fn check(&self) -> bool {
        self.authenticator.is_authenticated()
    }

    pub fn authenticator(&self) -> &dyn BiometricAuthenticator {
        self.authenticator.as_ref()
    }

    pub fn config(&self) -> &BiometricConfig {
        &self.config
    }
}

/// 生成一个新的设备 ID（UUID v4）
pub fn new_device_id() -> String {
    Uuid::new_v4().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utc(secs: i64) -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::from_timestamp(secs, 0).unwrap()
    }

    #[test]
    fn test_biometric_config_defaults() {
        let cfg = BiometricConfig::default();
        assert_eq!(cfg.validity_secs, DEFAULT_VALIDITY_SECS);
        assert_eq!(cfg.kind, BiometricKind::None);
        assert!(cfg.allow_password_fallback);
    }

    #[test]
    fn test_biometric_config_builder() {
        let cfg = BiometricConfig::new(BiometricKind::FaceId)
            .with_validity_secs(120)
            .with_password_fallback(false);
        assert_eq!(cfg.validity_secs, 120);
        assert_eq!(cfg.kind, BiometricKind::FaceId);
        assert!(!cfg.allow_password_fallback);
    }

    #[test]
    fn test_biometric_kind_availability() {
        assert!(BiometricKind::FaceId.is_available());
        assert!(BiometricKind::Tpm.is_available());
        assert!(!BiometricKind::None.is_available());
    }

    #[test]
    fn test_mock_authenticate_success() {
        let auth = MockBiometricAuthenticator::new(BiometricConfig::new(BiometricKind::TouchId));
        auth.set_next_success(true);
        let status = auth.authenticate_at(utc(1000)).unwrap();
        assert_eq!(
            status,
            BiometricStatus::Authenticated {
                at: utc(1000),
                kind: BiometricKind::TouchId
            }
        );
        assert!(auth.is_authenticated_at(utc(1000)));
        assert!(auth.is_authenticated_at(utc(1000 + 299))); // 仍有效
    }

    #[test]
    fn test_mock_authenticate_failure() {
        let auth = MockBiometricAuthenticator::new(BiometricConfig::new(BiometricKind::FaceId));
        auth.set_next_success(false);
        let status = auth.authenticate().unwrap();
        assert_eq!(status, BiometricStatus::Failed);
        assert!(!auth.is_authenticated());
    }

    #[test]
    fn test_validity_window_not_expired() {
        let auth = MockBiometricAuthenticator::new(
            BiometricConfig::new(BiometricKind::FaceId).with_validity_secs(300),
        );
        auth.set_next_success(true);
        auth.authenticate_at(utc(1000)).unwrap();
        // 在有效期内
        assert!(auth.is_authenticated_at(utc(1000)));
        assert!(auth.is_authenticated_at(utc(1299)));
        assert!(auth.is_authenticated_at(utc(1300))); // 边界：恰好 300 秒
    }

    #[test]
    fn test_validity_window_expired() {
        let auth = MockBiometricAuthenticator::new(
            BiometricConfig::new(BiometricKind::FaceId).with_validity_secs(300),
        );
        auth.set_next_success(true);
        auth.authenticate_at(utc(1000)).unwrap();
        // 超出有效期
        assert!(!auth.is_authenticated_at(utc(1301)));
        assert!(!auth.is_authenticated_at(utc(2000)));
    }

    #[test]
    fn test_invalidate_resets_status() {
        let auth = MockBiometricAuthenticator::new(BiometricConfig::new(BiometricKind::TouchId));
        auth.set_next_success(true);
        auth.authenticate_at(utc(1000)).unwrap();
        assert!(auth.is_authenticated_at(utc(1000)));
        auth.invalidate();
        assert!(!auth.is_authenticated_at(utc(1000)));
        assert_eq!(auth.current_status(), BiometricStatus::Unauthenticated);
    }

    #[test]
    fn test_unavailable_status() {
        let auth = MockBiometricAuthenticator::new(BiometricConfig::new(BiometricKind::Tpm));
        auth.set_available(false);
        let status = auth.authenticate().unwrap();
        assert_eq!(status, BiometricStatus::Unavailable);
        assert!(!auth.is_available());
        assert!(!auth.is_authenticated());
    }

    #[test]
    fn test_protector_ensure_authenticated_when_valid() {
        let auth = MockBiometricAuthenticator::new(BiometricConfig::new(BiometricKind::FaceId));
        auth.set_next_success(true);
        let protector = BiometricProtector::new(Box::new(auth), BiometricConfig::default());
        // 首次需要认证
        protector.ensure_authenticated().unwrap();
        // 第二次应直接放行（仍在有效期内）
        protector.ensure_authenticated().unwrap();
    }

    #[test]
    fn test_protector_rejects_when_authentication_fails() {
        let auth = MockBiometricAuthenticator::new(BiometricConfig::new(BiometricKind::FaceId));
        auth.set_next_success(false);
        let cfg = BiometricConfig::new(BiometricKind::FaceId).with_password_fallback(false);
        let protector = BiometricProtector::new(Box::new(auth), cfg);
        assert!(protector.ensure_authenticated().is_err());
    }

    #[test]
    fn test_protector_falls_back_to_password_when_unavailable() {
        let auth = MockBiometricAuthenticator::new(BiometricConfig::new(BiometricKind::None));
        auth.set_available(false);
        let cfg = BiometricConfig::new(BiometricKind::None).with_password_fallback(true);
        let protector = BiometricProtector::new(Box::new(auth), cfg);
        // 硬件不可用但允许口令回退 -> 放行
        assert!(protector.ensure_authenticated().is_ok());
    }

    #[test]
    fn test_protector_no_fallback_when_unavailable() {
        let auth = MockBiometricAuthenticator::new(BiometricConfig::new(BiometricKind::None));
        auth.set_available(false);
        let cfg = BiometricConfig::new(BiometricKind::None).with_password_fallback(false);
        let protector = BiometricProtector::new(Box::new(auth), cfg);
        assert!(protector.ensure_authenticated().is_err());
    }

    #[test]
    fn test_new_device_id_unique() {
        let id1 = new_device_id();
        let id2 = new_device_id();
        assert_ne!(id1, id2);
    }
}
