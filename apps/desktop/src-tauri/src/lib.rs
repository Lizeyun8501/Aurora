//! Aurora Desktop — Tauri v2 Platform Adapter
//!
//! 对应 V19 §30 平台适配要求，提供：
//! - Tauri command 薄层（将前端调用路由到 aurora-core）
//! - DesktopPlatform Trait 实现（菜单、托盘、快捷键、剪贴板）
//! - 启动时 AppCore 初始化与恢复

use std::sync::{Arc, Mutex};

use aurora_core::app_core::{AppCore, AppCoreBuilder};
use tracing::{error, info, warn};

/// 桌面端应用状态（全局单例）。
static APP_STATE: Mutex<Option<Arc<AppCore>>> = Mutex::new(None);

/// 初始化桌面应用核心。
/// 由 `main.rs` 在 Tauri 启动流程中调用。
pub fn run() {
    // 初始化 tracing
    tracing_subscriber::fmt::init();
    info!("Aurora Desktop starting");

    // TODO: 当 AppCoreBuilder 的所有依赖就绪后启用真实初始化
    // let data_dir = tauri::api::path::app_data_dir(&tauri::Config::default())
    //     .unwrap_or_else(|| std::env::temp_dir().join("aurora"));
    //
    // let core = AppCoreBuilder::new(data_dir)
    //     .build()
    //     .expect("AppCore init failed");
    // core.startup().expect("AppCore startup failed");
    //
    // *APP_STATE.lock().unwrap() = Some(Arc::new(core));

    // 当前阶段：占位，打印启动信息
    info!("Aurora Desktop placeholder started (AppCore integration pending)");
}

/// 获取当前 AppCore 实例（用于 command handler 内部）。
fn with_core<F, R>(f: F) -> Result<R, String>
where
    F: FnOnce(&Arc<AppCore>) -> Result<R, String>,
{
    let guard = APP_STATE.lock().map_err(|e| format!("mutex poisoned: {}", e))?;
    match guard.as_ref() {
        Some(core) => f(core),
        None => Err("AppCore not initialized".into()),
    }
}

// ── Tauri Commands (§30 平台适配) ─────────────────────────────

/// 创建新笔记。
#[tauri::command]
pub fn cmd_create_note(title: String) -> Result<String, String> {
    with_core(|_core| {
        // TODO: 接入 core.note_service().create(&title)
        let id = uuid::Uuid::new_v4().to_string();
        info!(note_id = %id, title, "note created via desktop command");
        Ok(id)
    })
}

/// 获取笔记内容。
#[tauri::command]
pub fn cmd_get_note(note_id: String) -> Result<serde_json::Value, String> {
    with_core(|_core| {
        // TODO: 接入 core.note_service().get(&note_id)
        Ok(serde_json::json!({
            "id": note_id,
            "title": "Placeholder",
            "content": "",
            "content_type": "markdown",
        }))
    })
}

/// 更新笔记。
#[tauri::command]
pub fn cmd_update_note(
    note_id: String,
    title: Option<String>,
    content: Option<String>,
) -> Result<(), String> {
    with_core(|_core| {
        info!(note_id = %note_id, ?title, ?content, "note updated via desktop command");
        Ok(())
    })
}

/// 删除笔记。
#[tauri::command]
pub fn cmd_delete_note(note_id: String) -> Result<(), String> {
    with_core(|_core| {
        info!(note_id = %note_id, "note deleted via desktop command");
        Ok(())
    })
}

/// 搜索笔记。
#[tauri::command]
pub fn cmd_search_notes(query: String) -> Result<Vec<serde_json::Value>, String> {
    with_core(|_core| {
        info!(query, "search notes via desktop command");
        // TODO: 接入 core.search().fts_search(&query)
        Ok(vec![])
    })
}

/// 获取应用状态摘要（健康检查）。
#[tauri::command]
pub fn cmd_app_status() -> Result<serde_json::Value, String> {
    with_core(|_core| {
        Ok(serde_json::json!({
            "status": "healthy",
            "platform": "desktop",
        }))
    })
}

// ── DesktopPlatform Trait (§30) ───────────────────────────────

/// 桌面端平台能力 Trait。
/// 由 AppCore 初始化时注入，提供原生桌面功能（菜单、托盘、剪贴板、快捷键）。
pub trait DesktopPlatform: Send + Sync {
    /// 设置应用托盘图标与菜单。
    fn set_tray(&self, icon_path: &str, menu_items: Vec<TrayMenuItem>);
    /// 注册全局快捷键。
    fn register_shortcut(&self, accelerator: &str, callback: Box<dyn Fn() + Send>);
    /// 设置应用菜单。
    fn set_menu(&self, menu_spec: &str);
    /// 读取剪贴板内容。
    fn clipboard_read(&self) -> Result<String, String>;
    /// 写入剪贴板内容。
    fn clipboard_write(&self, text: &str) -> Result<(), String>;
    /// 触发原生通知。
    fn notify(&self, title: &str, body: &str);
}

/// 托盘菜单项。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TrayMenuItem {
    pub id: String,
    pub label: String,
    pub accelerator: Option<String>,
}

/// 默认 DesktopPlatform 实现（基于 Tauri API）。
pub struct TauriDesktopPlatform;

impl TauriDesktopPlatform {
    pub fn new() -> Self {
        Self
    }
}

impl DesktopPlatform for TauriDesktopPlatform {
    fn set_tray(&self, icon_path: &str, menu_items: Vec<TrayMenuItem>) {
        info!(icon_path, ?menu_items, "set_tray requested");
        // TODO: 接入 tauri::SystemTray
    }

    fn register_shortcut(&self, accelerator: &str, _callback: Box<dyn Fn() + Send>) {
        info!(accelerator, "register_shortcut requested");
        // TODO: 接入 tauri::GlobalShortcutManager
    }

    fn set_menu(&self, menu_spec: &str) {
        info!(menu_spec, "set_menu requested");
        // TODO: 接入 tauri::Menu
    }

    fn clipboard_read(&self) -> Result<String, String> {
        // TODO: 接入 tauri::api::clipboard::read_text
        Ok(String::new())
    }

    fn clipboard_write(&self, text: &str) -> Result<(), String> {
        info!(len = text.len(), "clipboard_write requested");
        // TODO: 接入 tauri::api::clipboard::write_text
        Ok(())
    }

    fn notify(&self, title: &str, body: &str) {
        info!(title, body, "notify requested");
        // TODO: 接入 tauri::api::notification::Notification
    }
}
