//! DK-17 S1：备份 command 层（手动触发 + 状态查询；UI 入口 S2）。

use aurora_bootstrap::BackupStatus;

use crate::sync_commands::get_booted;

/// 立即执行一次备份（不受 boot 水位限制）。
#[tauri::command]
pub async fn cmd_backup_now() -> Result<Option<aurora_core::backup::BackupReport>, String> {
    get_booted()?.run_backup_now().map_err(|e| e.to_string())
}

/// 备份状态（水位 / SHA / 错误）。
#[tauri::command]
pub async fn cmd_backup_status() -> Result<BackupStatus, String> {
    get_booted()?.backup_status().map_err(|e| e.to_string())
}
