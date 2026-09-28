//! DK-10 切片 3：AI 云策略 command 层（desktop UI 开关后端面）。
//!
//! 读写经 `BootedApp` 同源语义：KV `ws-policy:{id}` 权威（崩溃安全，重启
//! 预载恢复）+ `WorkspaceConfigResolver` 内存直写即时生效。workspace_id
//! 缺省 = `AI_DEFAULT_WORKSPACE_ID`（单工作区口径；多工作区立项后由前端
//! 传实际 id，命令签名不变）。

use aurora_ai::policy::WorkspacePolicy;
use aurora_bootstrap::AI_DEFAULT_WORKSPACE_ID;

use crate::sync_commands::get_booted;

fn ws_or_default(workspace_id: Option<String>) -> String {
    workspace_id.unwrap_or_else(|| AI_DEFAULT_WORKSPACE_ID.into())
}

/// 查询工作区 AI 云策略（"allow" | "deny"）。
#[tauri::command]
pub async fn cmd_get_ai_cloud_policy(workspace_id: Option<String>) -> Result<String, String> {
    let ws = ws_or_default(workspace_id);
    let booted = get_booted()?;
    Ok(match booted.ai_cloud_policy(&ws) {
        WorkspacePolicy::AllowCloud => "allow",
        WorkspacePolicy::DenyCloud => "deny",
    }
    .into())
}

/// 设置工作区 AI 云策略（deny = true 禁止云端出网）。
#[tauri::command]
pub async fn cmd_set_ai_cloud_policy(
    workspace_id: Option<String>,
    deny: bool,
) -> Result<(), String> {
    let ws = ws_or_default(workspace_id);
    get_booted()?
        .set_ai_cloud_policy(&ws, deny)
        .map_err(|e| e.to_string())
}
