//! 插件热更新 (Hot Update)
//!
//! 四阶段热更新流程：后台预加载 → 用户确认切换 → 失败自动回滚 → 金丝雀灰度发布。
//!
//! # 流程
//! 1. `preload`：后台下载新版本字节码，完成后进入 `Pending` 等待用户确认。
//! 2. `confirm_switch`：用户确认后切换到新版本；若切换失败则自动 `rollback`。
//! 3. `rollback`：恢复旧版本字节码与版本号，状态置为 `RolledBack`。
//! 4. 金丝雀：`confirm_switch(canary=true)` 标记为灰度发布，可按比例放量。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

/// 热更新状态。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum UpdateState {
    /// 后台预加载中
    Preloading,
    /// 预加载完成，等待用户确认
    Pending,
    /// 切换中
    Switching,
    /// 切换完成
    Completed,
    /// 已回滚
    RolledBack,
    /// 失败（回滚后仍不可用）
    Failed,
}

impl UpdateState {
    pub fn as_str(&self) -> &'static str {
        match self {
            UpdateState::Preloading => "preloading",
            UpdateState::Pending => "pending",
            UpdateState::Switching => "switching",
            UpdateState::Completed => "completed",
            UpdateState::RolledBack => "rolled_back",
            UpdateState::Failed => "failed",
        }
    }
}

/// 热更新结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateResult {
    pub plugin_id: String,
    pub from_version: String,
    pub to_version: String,
    pub state: UpdateState,
    pub message: String,
    pub canary: bool,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl UpdateResult {
    fn new(
        plugin_id: impl Into<String>,
        from_version: impl Into<String>,
        to_version: impl Into<String>,
        state: UpdateState,
        message: impl Into<String>,
        canary: bool,
    ) -> Self {
        Self {
            plugin_id: plugin_id.into(),
            from_version: from_version.into(),
            to_version: to_version.into(),
            state,
            message: message.into(),
            canary,
            timestamp: chrono::Utc::now(),
        }
    }
}

/// 单个插件的热更新作业（内部）。
#[derive(Debug, Clone)]
struct UpdateJob {
    from_version: String,
    to_version: String,
    state: UpdateState,
    canary: bool,
    /// 旧版本字节码（用于回滚）
    old_blob: Vec<u8>,
    /// 新版本字节码
    new_blob: Vec<u8>,
    /// 测试钩子：标记切换应失败以触发回滚
    switch_should_fail: bool,
}

/// 已安装插件记录（内部）。
#[derive(Debug, Clone)]
struct Installed {
    version: String,
    blob: Vec<u8>,
}

/// 热更新管理器。
pub struct HotUpdateManager {
    jobs: Arc<RwLock<HashMap<String, UpdateJob>>>,
    installed: Arc<RwLock<HashMap<String, Installed>>>,
    /// 金丝雀灰度比例（0–100）
    canary_percentage: Arc<RwLock<u8>>,
}

impl Default for HotUpdateManager {
    fn default() -> Self {
        Self::new()
    }
}

impl HotUpdateManager {
    pub fn new() -> Self {
        Self {
            jobs: Arc::new(RwLock::new(HashMap::new())),
            installed: Arc::new(RwLock::new(HashMap::new())),
            canary_percentage: Arc::new(RwLock::new(0)),
        }
    }

    /// 安装某插件的初始版本。
    pub fn install(&self, plugin_id: impl Into<String>, version: impl Into<String>, blob: Vec<u8>) {
        let id = plugin_id.into();
        let ver = version.into();
        info!("hot update: install {} v{}", id, ver);
        self.installed
            .write()
            .insert(id, Installed { version: ver, blob });
    }

    /// 设置金丝雀灰度比例（0–100）。
    pub fn set_canary_percentage(&self, percentage: u8) {
        *self.canary_percentage.write() = percentage.min(100);
    }

    /// 当前金丝雀比例。
    pub fn canary_percentage(&self) -> u8 {
        *self.canary_percentage.read()
    }

    /// 当前已安装版本。
    pub fn current_version(&self, plugin_id: &str) -> Option<String> {
        self.installed
            .read()
            .get(plugin_id)
            .map(|i| i.version.clone())
    }

    /// 当前已安装字节码长度（便于测试断言）。
    pub fn current_blob_len(&self, plugin_id: &str) -> Option<usize> {
        self.installed.read().get(plugin_id).map(|i| i.blob.len())
    }

    /// 某插件的热更新状态。
    pub fn state(&self, plugin_id: &str) -> Option<UpdateState> {
        self.jobs.read().get(plugin_id).map(|j| j.state)
    }

    /// 是否为金丝雀发布。
    pub fn is_canary(&self, plugin_id: &str) -> bool {
        self.jobs
            .read()
            .get(plugin_id)
            .map(|j| j.canary)
            .unwrap_or(false)
    }

    /// 测试钩子：标记下次切换应失败（用于验证回滚路径）。
    pub fn simulate_switch_failure(&self, plugin_id: &str) -> Result<(), crate::Error> {
        let mut jobs = self.jobs.write();
        let job = jobs
            .get_mut(plugin_id)
            .ok_or_else(|| crate::Error::NotFound(format!("no update job: {}", plugin_id)))?;
        job.switch_should_fail = true;
        Ok(())
    }

    /// 后台预加载新版本。模拟异步下载，完成后状态为 `Pending`。
    pub async fn preload(
        &self,
        plugin_id: impl Into<String>,
        new_version: impl Into<String>,
        new_blob: Vec<u8>,
    ) -> Result<UpdateResult, crate::Error> {
        let id = plugin_id.into();
        let new_version = new_version.into();
        let installed = self
            .installed
            .read()
            .get(&id)
            .cloned()
            .ok_or_else(|| crate::Error::NotFound(format!("plugin not installed: {}", id)))?;

        info!(
            "hot update: preloading {} v{} -> v{}",
            id, installed.version, new_version
        );
        // 标记 Preloading
        {
            let mut jobs = self.jobs.write();
            jobs.insert(
                id.clone(),
                UpdateJob {
                    from_version: installed.version.clone(),
                    to_version: new_version.clone(),
                    state: UpdateState::Preloading,
                    canary: false,
                    old_blob: installed.blob.clone(),
                    new_blob: new_blob.clone(),
                    switch_should_fail: false,
                },
            );
        }
        // 模拟后台下载 I/O
        tokio::time::sleep(Duration::from_millis(1)).await;

        // 预加载完成 → Pending
        self.jobs.write().get_mut(&id).unwrap().state = UpdateState::Pending;
        debug!("hot update: {} preload complete -> pending", id);
        Ok(UpdateResult::new(
            id,
            installed.version,
            new_version,
            UpdateState::Pending,
            "preload complete, awaiting confirmation",
            false,
        ))
    }

    /// 用户确认切换到预加载的新版本。
    ///
    /// - `canary=true` 标记为金丝雀灰度发布。
    /// - 切换失败时自动回滚并返回 `RolledBack` 结果。
    pub fn confirm_switch(
        &self,
        plugin_id: &str,
        canary: bool,
    ) -> Result<UpdateResult, crate::Error> {
        let (from_version, to_version, new_blob, should_fail) = {
            let mut jobs = self.jobs.write();
            let job = jobs
                .get_mut(plugin_id)
                .ok_or_else(|| crate::Error::NotFound(format!("no update job: {}", plugin_id)))?;
            if job.state != UpdateState::Pending {
                return Err(crate::Error::HotUpdate(format!(
                    "cannot switch from state {:?} (must be Pending)",
                    job.state
                )));
            }
            job.state = UpdateState::Switching;
            job.canary = canary;
            (
                job.from_version.clone(),
                job.to_version.clone(),
                job.new_blob.clone(),
                job.switch_should_fail,
            )
        };

        info!(
            "hot update: switching {} v{} -> v{} (canary={})",
            plugin_id, from_version, to_version, canary
        );

        if should_fail {
            warn!("hot update: switch failed for {}, rolling back", plugin_id);
            self.rollback(plugin_id)?;
            return Ok(UpdateResult::new(
                plugin_id,
                from_version,
                to_version,
                UpdateState::RolledBack,
                "switch failed, rolled back to previous version",
                canary,
            ));
        }

        // 应用新版本
        self.installed.write().insert(
            plugin_id.to_string(),
            Installed {
                version: to_version.clone(),
                blob: new_blob,
            },
        );
        self.jobs.write().get_mut(plugin_id).unwrap().state = UpdateState::Completed;
        info!("hot update: {} switched to v{}", plugin_id, to_version);
        Ok(UpdateResult::new(
            plugin_id,
            from_version,
            to_version,
            UpdateState::Completed,
            "switch completed",
            canary,
        ))
    }

    /// 回滚到旧版本。
    pub fn rollback(&self, plugin_id: &str) -> Result<UpdateResult, crate::Error> {
        let (from_version, to_version, old_blob, canary) = {
            let mut jobs = self.jobs.write();
            let job = jobs
                .get_mut(plugin_id)
                .ok_or_else(|| crate::Error::NotFound(format!("no update job: {}", plugin_id)))?;
            if job.state == UpdateState::RolledBack || job.state == UpdateState::Completed {
                // Completed 也可回滚（用户主动回退）
            }
            (
                job.from_version.clone(),
                job.to_version.clone(),
                job.old_blob.clone(),
                job.canary,
            )
        };

        // 恢复旧版本
        self.installed.write().insert(
            plugin_id.to_string(),
            Installed {
                version: from_version.clone(),
                blob: old_blob,
            },
        );
        self.jobs.write().get_mut(plugin_id).unwrap().state = UpdateState::RolledBack;
        warn!(
            "hot update: {} rolled back v{} -> v{}",
            plugin_id, to_version, from_version
        );
        Ok(UpdateResult::new(
            plugin_id,
            to_version,
            from_version,
            UpdateState::RolledBack,
            "rolled back to previous version",
            canary,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn installed_manager() -> HotUpdateManager {
        let mgr = HotUpdateManager::new();
        mgr.install("p1", "1.0.0", vec![0u8; 100]);
        mgr
    }

    #[tokio::test]
    async fn test_hot_update_preload_to_pending() {
        let mgr = installed_manager();
        let result = mgr.preload("p1", "1.1.0", vec![1u8; 200]).await.unwrap();
        assert_eq!(result.state, UpdateState::Pending);
        assert_eq!(result.from_version, "1.0.0");
        assert_eq!(result.to_version, "1.1.0");
        assert_eq!(mgr.state("p1"), Some(UpdateState::Pending));
        // 预加载期间不应改变已安装版本
        assert_eq!(mgr.current_version("p1"), Some("1.0.0".to_string()));
    }

    #[tokio::test]
    async fn test_hot_update_confirm_switch_completed() {
        let mgr = installed_manager();
        mgr.preload("p1", "2.0.0", vec![2u8; 50]).await.unwrap();

        let result = mgr.confirm_switch("p1", false).unwrap();
        assert_eq!(result.state, UpdateState::Completed);
        assert_eq!(mgr.state("p1"), Some(UpdateState::Completed));
        assert_eq!(mgr.current_version("p1"), Some("2.0.0".to_string()));
        assert_eq!(mgr.current_blob_len("p1"), Some(50));
        assert!(!mgr.is_canary("p1"));
    }

    #[tokio::test]
    async fn test_hot_update_switch_failure_rollback() {
        let mgr = installed_manager();
        mgr.preload("p1", "2.0.0", vec![2u8; 50]).await.unwrap();
        mgr.simulate_switch_failure("p1").unwrap();

        let result = mgr.confirm_switch("p1", false).unwrap();
        assert_eq!(result.state, UpdateState::RolledBack);
        assert_eq!(mgr.state("p1"), Some(UpdateState::RolledBack));
        // 回滚后版本应恢复为旧版本
        assert_eq!(mgr.current_version("p1"), Some("1.0.0".to_string()));
        assert_eq!(mgr.current_blob_len("p1"), Some(100));
    }

    #[tokio::test]
    async fn test_hot_update_canary_release() {
        let mgr = installed_manager();
        mgr.preload("p1", "2.0.0", vec![2u8; 50]).await.unwrap();
        let result = mgr.confirm_switch("p1", true).unwrap();
        assert_eq!(result.state, UpdateState::Completed);
        assert!(result.canary);
        assert!(mgr.is_canary("p1"));
    }

    #[tokio::test]
    async fn test_hot_update_canary_percentage() {
        let mgr = HotUpdateManager::new();
        assert_eq!(mgr.canary_percentage(), 0);
        mgr.set_canary_percentage(30);
        assert_eq!(mgr.canary_percentage(), 30);
        mgr.set_canary_percentage(150);
        assert_eq!(mgr.canary_percentage(), 100);
    }

    #[tokio::test]
    async fn test_hot_update_rollback_after_completed() {
        let mgr = installed_manager();
        mgr.preload("p1", "2.0.0", vec![2u8; 50]).await.unwrap();
        mgr.confirm_switch("p1", false).unwrap();
        assert_eq!(mgr.current_version("p1"), Some("2.0.0".to_string()));

        // 用户主动回滚
        let result = mgr.rollback("p1").unwrap();
        assert_eq!(result.state, UpdateState::RolledBack);
        assert_eq!(mgr.current_version("p1"), Some("1.0.0".to_string()));
        assert_eq!(mgr.current_blob_len("p1"), Some(100));
    }

    #[tokio::test]
    async fn test_hot_update_confirm_switch_wrong_state() {
        let mgr = installed_manager();
        // 没有 preload 直接 confirm_switch 应失败
        let err = mgr.confirm_switch("p1", false).unwrap_err();
        assert!(matches!(err, crate::Error::NotFound(_)));

        mgr.preload("p1", "2.0.0", vec![2u8; 50]).await.unwrap();
        mgr.confirm_switch("p1", false).unwrap();
        // 已 Completed 再次 confirm_switch 应失败（状态非 Pending）
        let err = mgr.confirm_switch("p1", false).unwrap_err();
        assert!(matches!(err, crate::Error::HotUpdate(_)));
    }

    #[tokio::test]
    async fn test_hot_update_preload_not_installed() {
        let mgr = HotUpdateManager::new();
        let err = mgr.preload("ghost", "1.0.0", vec![]).await.unwrap_err();
        assert!(matches!(err, crate::Error::NotFound(_)));
    }

    #[test]
    fn test_update_state_as_str() {
        assert_eq!(UpdateState::Preloading.as_str(), "preloading");
        assert_eq!(UpdateState::Pending.as_str(), "pending");
        assert_eq!(UpdateState::RolledBack.as_str(), "rolled_back");
        assert_eq!(UpdateState::Completed.as_str(), "completed");
    }

    #[test]
    fn test_install_and_current_version() {
        let mgr = HotUpdateManager::new();
        mgr.install("p1", "3.1.0", vec![1, 2, 3]);
        assert_eq!(mgr.current_version("p1"), Some("3.1.0".to_string()));
        assert_eq!(mgr.current_blob_len("p1"), Some(3));
        assert_eq!(mgr.current_version("ghost"), None);
    }
}
