//! A-08: 桌面日历命令面——与移动 FFI 同源 core（event_store/task_store/
//! calendar_vm）,双端同一 VM 同一数据。
//!
//! 通路: Tauri IPC → core（kv 直注+event_bus）——ADR-003 日历域例外
//! 同 event_store 裁决（2026-10-29: 无 seal/attachments 依赖）。
//!
//! 纪律:
//! - upsert/list/delete_event 走 event_store 原语（移动端 FFI 同款——
//!   双端行为对齐铁律 5）
//! - reschedule_task → task_store（**发 TaskRescheduled**——投影/双端同步）
//! - fail-closed: 全链错误字符串化透传 UI,禁止静默假成功

use crate::get_core;
use aurora_core::task_store::CalendarTask;

/// A-08: 双端统一日历 VM（events+tasks 全量——前端只渲染）。
#[tauri::command]
pub async fn cmd_calendar_vm() -> Result<aurora_core::calendar_vm::CalendarVm, String> {
    let core = get_core()?;
    aurora_core::calendar_vm::build_calendar_vm(core.kv_store.as_ref())
        .await
        .map_err(|e| e.to_string())
}

/// 事件全量列表（start 升序——移动端 FFI list_events_json 同源）。
#[tauri::command]
pub async fn cmd_list_events() -> Result<Vec<aurora_entity::Event>, String> {
    let core = get_core()?;
    aurora_core::event_store::list_events(core.kv_store.as_ref())
        .await
        .map_err(|e| e.to_string())
}

/// 事件新增/更新（id 空则 core 建 UUID;校验 fail-closed 同移动）。
#[tauri::command]
pub async fn cmd_upsert_event(event_json: String) -> Result<String, String> {
    let core = get_core()?;
    let ev: aurora_entity::Event =
        serde_json::from_str(&event_json).map_err(|e| format!("event json 解析失败: {e}"))?;
    aurora_core::event_store::upsert_event(core.kv_store.as_ref(), &ev)
        .await
        .map_err(|e| e.to_string())
}

/// 事件删除。
#[tauri::command]
pub async fn cmd_delete_event(event_id: String) -> Result<(), String> {
    let core = get_core()?;
    aurora_core::event_store::delete_event(core.kv_store.as_ref(), &event_id)
        .await
        .map_err(|e| e.to_string())
}

/// A-08: 任务改期——**WritePath 事件面**（TaskRescheduled,Medium 持久化）。
/// 桌面拖拽改期入口;幽灵任务 fail-closed。
#[tauri::command]
pub async fn cmd_reschedule_task(task_id: String, new_due: Option<String>) -> Result<(), String> {
    let core = get_core()?;
    aurora_core::task_store::reschedule_task(
        core.kv_store.as_ref(),
        &core.event_bus,
        &task_id,
        new_due,
    )
    .await
    .map_err(|e| e.to_string())
}

/// A-08: 任务全量镜像同步（移动端双写面同款——增删改一次覆盖,差集删除）。
#[tauri::command]
pub async fn cmd_sync_tasks(tasks_json: String) -> Result<(usize, usize), String> {
    let core = get_core()?;
    let tasks: Vec<CalendarTask> =
        serde_json::from_str(&tasks_json).map_err(|e| format!("tasks json 解析失败: {e}"))?;
    aurora_core::task_store::sync_tasks(core.kv_store.as_ref(), &tasks)
        .await
        .map_err(|e| e.to_string())
}
