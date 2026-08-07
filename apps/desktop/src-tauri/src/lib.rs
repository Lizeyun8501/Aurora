//! Aurora Desktop — Tauri v2 Platform Adapter
//!
//! 对应 V19 §30 平台适配要求，提供：
//! - Tauri command 薄层（将前端调用路由到 aurora-core）
//! - DesktopPlatform Trait 实现（菜单、托盘、快捷键、剪贴板）
//! - 启动时 AppCore 初始化与恢复（V19 §36.1）
//!
//! # 启动流程（V19 §36.1 + ARCH-003，装配复用 [`aurora_bootstrap`]）
//! 1. 确定 data_dir（用户库目录）
//! 2. `aurora_bootstrap::bootstrap()`：迁移 + DEK 保险库 + AppCore DI 注入 + startup
//! 3. 存入全局状态供 command handler 使用，并注册 Tauri 平台能力
//!
//! # E2EE 说明
//! 笔记 JSON 经 [`LocalDekVault`]（32 字节随机 DEK + AES-256-GCM）加密后写入
//! KVStore，满足 V19 §10「明文 → DEK 加密 → 存储密文」；本地全文检索索引
//! （Tantivy）按 V19 设计保留明文，仅存在于本机。
//! 过渡方案说明：DEK 当前以本地文件保管（Unix 0600），生产应迁移至
//! `KeyHierarchy` 口令解锁 + OS 安全存储（DPAPI / Keychain）。

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use aurora_core::app_core::AppCore;
use aurora_core::traits::crypto_provider::CryptoProvider;
use aurora_core::traits::kv_store::KVStore;
use aurora_core::traits::search_backend::{NoteMetadata, SearchBackend};
use aurora_security::LocalDekVault;
use tracing::{info, warn};

// ── 启动期常量与全局状态 ─────────────────────────────

/// 桌面端默认数据目录名。
const AURORA_DIR_NAME: &str = "aurora";
/// 默认工作区 ID（单工作区模式；多工作区接入后改为按用户选择注入）。
const DEFAULT_WORKSPACE_ID: &str = "default";

/// 桌面端应用核心（全局单例）。
static APP_STATE: Mutex<Option<Arc<AppCore>>> = Mutex::new(None);
/// 本地 DEK 保险库（全局单例）。
static VAULT_STATE: Mutex<Option<Arc<LocalDekVault>>> = Mutex::new(None);

/// 获取用户数据目录路径。
///
/// 平台约定：
/// - Linux: `~/.local/share/aurora/`
/// - macOS: `~/Library/Application Support/aurora/`
/// - Windows: `%APPDATA%\aurora\`
fn get_data_dir() -> PathBuf {
    if let Some(dir) = dirs_next::data_dir() {
        dir.join(AURORA_DIR_NAME)
    } else {
        std::env::temp_dir().join(AURORA_DIR_NAME)
    }
}

/// 获取 AppCore 实例的 Arc 引用（不持有锁，可安全跨 .await）。
fn get_core() -> Result<Arc<AppCore>, String> {
    let guard = APP_STATE
        .lock()
        .map_err(|e| format!("mutex poisoned: {}", e))?;
    guard
        .as_ref()
        .cloned()
        .ok_or_else(|| "AppCore not initialized".into())
}

/// 获取本地 DEK 保险库引用。
fn get_vault() -> Result<Arc<LocalDekVault>, String> {
    let guard = VAULT_STATE
        .lock()
        .map_err(|e| format!("mutex poisoned: {}", e))?;
    guard
        .as_ref()
        .cloned()
        .ok_or_else(|| "vault not initialized".into())
}

fn box_err(e: impl std::fmt::Display) -> Box<dyn std::error::Error> {
    Box::<dyn std::error::Error>::from(e.to_string())
}

/// 初始化桌面应用并启动 Tauri 事件循环（V19 §36.1 真实启动入口）。
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            cmd_create_note,
            cmd_get_note,
            cmd_update_note,
            cmd_delete_note,
            cmd_search_notes,
            cmd_app_status,
        ])
        .setup(|app| {
            tracing_subscriber::fmt::init();
            info!("Aurora Desktop starting");

            let data_dir = get_data_dir();
            info!(data_dir = ?data_dir, "data directory ready");

            // 共享装配：迁移 + DEK 保险库 + AppCore DI 注入 + startup（V19 §36.1）
            let booted = aurora_bootstrap::bootstrap(&data_dir).map_err(box_err)?;
            *VAULT_STATE.lock().expect("VAULT_STATE mutex poisoned") = Some(booted.vault.clone());
            *APP_STATE.lock().expect("APP_STATE mutex poisoned") = Some(booted.core.clone());
            info!("AppCore startup complete");

            // 注册平台能力（托盘/快捷键/剪贴板/通知），供后续 command 使用
            app.manage(TauriDesktopPlatform::new(app.handle().clone()));

            info!("Aurora Desktop initialized and ready");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// ── 笔记编解码辅助（E2EE + 搜索索引同步） ──────────────

/// 解密笔记字节：优先按密文（bincode(Ciphertext)）解密；解密失败时兼容
/// 升级前的明文 JSON（旧版本无加密落库），保证存量数据不丢失。
fn unwrap_note_bytes(
    core: &AppCore,
    vault: &LocalDekVault,
    data: &[u8],
) -> Result<serde_json::Value, String> {
    match vault.decrypt(core.crypto.as_ref(), data) {
        Ok(plaintext) => serde_json::from_slice(&plaintext).map_err(|e| e.to_string()),
        Err(e) => match serde_json::from_slice::<serde_json::Value>(data) {
            Ok(v) => {
                warn!("note stored in legacy plaintext format; consider re-saving",);
                Ok(v)
            }
            Err(_) => Err(format!("note decrypt failed: {}", e)),
        },
    }
}

/// 将笔记明文加密为落库字节。
fn seal_note_bytes(
    core: &AppCore,
    vault: &LocalDekVault,
    note: &serde_json::Value,
) -> Result<Vec<u8>, String> {
    let payload = serde_json::to_vec(note).map_err(|e| e.to_string())?;
    vault
        .encrypt(core.crypto.as_ref(), &payload)
        .map_err(|e| e.to_string())
}

/// 同步笔记到本地全文检索索引（明文索引，V19 本地检索设计）。
async fn index_note_in_search(
    core: &AppCore,
    id: &str,
    note: &serde_json::Value,
) -> Result<(), String> {
    let title = note
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let content = note
        .get("content")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let metadata = NoteMetadata {
        title,
        tags: vec![],
        workspace_id: DEFAULT_WORKSPACE_ID.to_string(),
        updated_at: Some(chrono::Utc::now()),
    };
    core.search
        .index_note(id, &content, &metadata)
        .await
        .map_err(|e| e.to_string())
}

// ── Tauri Commands（§30 平台适配） ─────────────────────

/// 创建新笔记（加密存储 + 建立搜索索引）。
#[tauri::command]
pub async fn cmd_create_note(title: String) -> Result<String, String> {
    let core = get_core()?;
    let vault = get_vault()?;
    let id = uuid::Uuid::new_v4().to_string();
    let note = serde_json::json!({
        "id": id,
        "title": title,
        "content": "",
        "content_type": "markdown",
        "created_at": chrono::Utc::now().to_rfc3339(),
    });
    let sealed = seal_note_bytes(&core, &vault, &note)?;
    core.kv_store
        .set(&format!("note:{}", id), &sealed)
        .await
        .map_err(|e| e.to_string())?;
    index_note_in_search(&core, &id, &note).await?;
    info!(note_id = %id, "note created via desktop command");
    Ok(id)
}

/// 获取笔记内容（解密后返回）。
#[tauri::command]
pub async fn cmd_get_note(note_id: String) -> Result<serde_json::Value, String> {
    let core = get_core()?;
    let vault = get_vault()?;
    let key = format!("note:{}", note_id);
    let data = core
        .kv_store
        .get(&key)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("note not found: {}", note_id))?;
    unwrap_note_bytes(&core, &vault, &data)
}

/// 更新笔记（解密 → 修改 → 重新加密落库 + 更新索引）。
#[tauri::command]
pub async fn cmd_update_note(
    note_id: String,
    title: Option<String>,
    content: Option<String>,
) -> Result<(), String> {
    let core = get_core()?;
    let vault = get_vault()?;
    let key = format!("note:{}", note_id);
    let mut note = match core.kv_store.get(&key).await.map_err(|e| e.to_string())? {
        Some(data) => unwrap_note_bytes(&core, &vault, &data)?,
        None => serde_json::json!({"id": note_id, "content_type": "markdown"}),
    };
    if let Some(t) = title {
        note["title"] = serde_json::Value::String(t);
    }
    if let Some(c) = content {
        note["content"] = serde_json::Value::String(c);
    }
    note["updated_at"] = serde_json::Value::String(chrono::Utc::now().to_rfc3339());
    let sealed = seal_note_bytes(&core, &vault, &note)?;
    core.kv_store
        .set(&key, &sealed)
        .await
        .map_err(|e| e.to_string())?;
    index_note_in_search(&core, &note_id, &note).await?;
    info!(note_id = %note_id, "note updated via desktop command");
    Ok(())
}

/// 删除笔记（含搜索索引）。
#[tauri::command]
pub async fn cmd_delete_note(note_id: String) -> Result<(), String> {
    let core = get_core()?;
    let key = format!("note:{}", note_id);
    core.kv_store
        .delete(&key)
        .await
        .map_err(|e| e.to_string())?;
    core.search
        .remove_index(&note_id)
        .await
        .map_err(|e| e.to_string())?;
    info!(note_id = %note_id, "note deleted via desktop command");
    Ok(())
}

/// 搜索笔记（Tantivy 全文检索）。
#[tauri::command]
pub async fn cmd_search_notes(query: String) -> Result<Vec<serde_json::Value>, String> {
    let core = get_core()?;
    info!(query, "search notes via desktop command");
    let opts = aurora_core::traits::search_backend::SearchOptions::default();
    let result = core
        .search
        .search(&query, &opts)
        .await
        .map_err(|e| e.to_string())?;
    let results: Vec<serde_json::Value> = result
        .hits
        .into_iter()
        .map(|hit| {
            serde_json::json!({
                "doc_id": hit.note_id,
                "score": hit.score,
                "snippet": hit.snippet,
            })
        })
        .collect();
    Ok(results)
}

/// 获取应用状态摘要（健康检查）。
#[tauri::command]
pub fn cmd_app_status() -> Result<serde_json::Value, String> {
    let core = get_core()?;
    let crypto_version = core.crypto.algorithm_version();
    Ok(serde_json::json!({
        "status": "healthy",
        "platform": "desktop",
        "crypto_version": crypto_version,
        "workspace_id": DEFAULT_WORKSPACE_ID,
    }))
}

// ── DesktopPlatform Trait（§30） ───────────────────────

/// 桌面端平台能力 Trait。
/// 由 Tauri 初始化时注入应用状态，提供原生桌面功能（菜单、托盘、剪贴板、快捷键）。
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

/// Tauri v2 DesktopPlatform 实现（持有 AppHandle，接入应用生命周期）。
pub struct TauriDesktopPlatform {
    // 保留 AppHandle 供托盘/快捷键/通知等原生能力接入；
    // 当前实现为接口就绪 + 日志占位（见各方法 TODO）。
    #[allow(dead_code)]
    app: tauri::AppHandle,
}

impl TauriDesktopPlatform {
    pub fn new(app: tauri::AppHandle) -> Self {
        Self { app }
    }
}

impl DesktopPlatform for TauriDesktopPlatform {
    fn set_tray(&self, icon_path: &str, menu_items: Vec<TrayMenuItem>) {
        // TODO: 接入 tauri::tray::TrayIconBuilder（需 bundle icon 资源就绪后启用）
        info!(icon_path, ?menu_items, "set_tray requested");
    }

    fn register_shortcut(&self, accelerator: &str, _callback: Box<dyn Fn() + Send>) {
        // TODO: 接入 tauri-plugin-global-shortcut
        info!(accelerator, "register_shortcut requested");
    }

    fn set_menu(&self, menu_spec: &str) {
        // TODO: 接入 tauri::menu::Menu
        info!(menu_spec, "set_menu requested");
    }

    fn clipboard_read(&self) -> Result<String, String> {
        // TODO: 接入 tauri-plugin-clipboard-manager
        Ok(String::new())
    }

    fn clipboard_write(&self, text: &str) -> Result<(), String> {
        // TODO: 接入 tauri-plugin-clipboard-manager
        info!(len = text.len(), "clipboard_write requested");
        Ok(())
    }

    fn notify(&self, title: &str, body: &str) {
        // TODO: 接入 tauri-plugin-notification
        info!(title, body, "notify requested");
    }
}
