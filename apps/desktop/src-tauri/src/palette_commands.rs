//! C-12: 命令面板最近使用 IPC 面（core palette_recents 薄封装）。
//!
//! 与 calendar_commands 同款注入（get_core → core.kv_store 直注——ADR-003
//! 域例外:偏好域无 seal/attachments 依赖）。
//!
//! 通道语义（工单）:
//! - `get` 返回 JSON 串（`[{id,ts,count}]` 蛇形直通——前端 JSON.parse 消费）
//! - `track` 命中上报（id+ts;LRU 合并+计数在 core）
//! - 上报失败由前端 catch 静默（不阻断命令执行——工单验收约束）;
//!   本层错误原样透传给调用方决定（fail-closed 不吞错）

use aurora_core::palette_recents as recents;

/// 最近使用列表（JSON 串——工单字面返回形态）。
#[tauri::command]
pub async fn cmd_recent_commands_get() -> Result<String, String> {
    let core = crate::get_core()?;
    let list = recents::get_recent_commands(core.kv_store.as_ref())
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_string(&list).map_err(|e| e.to_string())
}

/// 记一次命令命中（LRU 合并同 id 计数+刷新 ts+移队首）。
#[tauri::command]
pub async fn cmd_recent_commands_track(id: String, ts: i64) -> Result<String, String> {
    let core = crate::get_core()?;
    let list = recents::track_recent_command(core.kv_store.as_ref(), &id, ts)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_string(&list).map_err(|e| e.to_string())
}
