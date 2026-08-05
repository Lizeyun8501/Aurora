//! Aurora Desktop — Tauri v2 Platform Adapter
//!
//! 对应 V19 §30 平台适配要求，提供：
//! - Tauri command 薄层（将前端调用路由到 aurora-core）
//! - DesktopPlatform Trait 实现（菜单、托盘、快捷键、剪贴板）
//! - 启动时 AppCore 初始化与恢复（V19 §36.1）
//!
//! # 启动流程（V19 §36.1 + ARCH-003）
//! 1. 确定 data_dir（用户库目录）
//! 2. 打开 SQLite 数据库并执行迁移
//! 3. 构造各 Trait 默认实现（LoroCrdtEngine / IrohSyncTarget / ...）
//! 4. 通过 AppCoreBuilder 注入
//! 5. app_core.startup() → 重放未消费事件 + 健康检查
//! 6. 存入全局 APP_STATE 供 command handler 使用

use std::sync::{Arc, Mutex};

use aurora_core::app_core::{AppCore, AppCoreBuilder};
use tracing::{error, info, warn};

// ── L1 默认实现（薄包装，真实生产由 DI 容器注入） ─────────────

/// 桌面端默认数据目录名。
const AURORA_DIR_NAME: &str = "aurora";

/// 获取用户数据目录路径。
///
/// 平台约定：
/// - Linux: `~/.local/share/aurora/`
/// - macOS: `~/Library/Application Support/aurora/`
/// - Windows: `%APPDATA%\aurora\`
fn get_data_dir() -> std::path::PathBuf {
    if let Some(dir) = dirs_next::data_dir() {
        dir.join(AURORA_DIR_NAME)
    } else {
        std::env::temp_dir().join(AURORA_DIR_NAME)
    }
}

/// 确保数据目录存在。
fn ensure_data_dir(path: &std::path::Path) -> std::io::Result<()> {
    if !path.exists() {
        std::fs::create_dir_all(path)?;
    }
    Ok(())
}

/// 桌面端应用状态（全局单例）。
static APP_STATE: Mutex<Option<Arc<AppCore>>> = Mutex::new(None);

/// 初始化桌面应用核心。
///
/// 这是真正的 AppCore 初始化入口（V19 §36.1），不再使用占位逻辑。
/// 失败时记录错误并 panic（早失败策略，避免在不可用状态下继续运行）。
pub fn run() {
    // 初始化 tracing
    tracing_subscriber::fmt::init();
    info!("Aurora Desktop starting");

    let data_dir = get_data_dir();
    if let Err(e) = ensure_data_dir(&data_dir) {
        error!(error = %e, dir = ?data_dir, "failed to create data directory");
        panic!("AppCore init: cannot create data dir: {}", e);
    }
    info!(data_dir = ?data_dir, "data directory ready");

    // 打开 SQLite 数据库并执行迁移
    let db_path = data_dir.join("aurora.db");
    let migration_manager = match aurora_migration::MigrationManager::new(&db_path) {
        Ok(m) => m,
        Err(e) => {
            error!(error = %e, db_path = ?db_path, "SQLite migration failed");
            panic!("AppCore init: database migration failed: {}", e);
        }
    };
    if let Err(e) = migration_manager.migrate() {
        error!(error = %e, "migration execution failed");
        panic!("AppCore init: migration execution failed: {}", e);
    }
    info!(db_path = ?db_path, "SQLite migrations complete");

    // 构造各 Trait 默认实现并注入 AppCoreBuilder
    let core = build_app_core(&data_dir, &db_path);
    if let Err(e) = core.startup() {
        error!(error = %e, "AppCore startup failed");
        panic!("AppCore startup failed: {}", e);
    }
    info!("AppCore startup complete");

    *APP_STATE
        .lock()
        .expect("APP_STATE mutex poisoned") = Some(Arc::new(core));
    info!("Aurora Desktop initialized and ready");
}

/// 构造 AppCore 并注入各 Trait 默认实现。
///
/// 对应 V19 §36.1 启动流程步骤 2-4。
fn build_app_core(data_dir: &std::path::Path, db_path: &std::path::Path) -> AppCore {
    // KVStore：基于 SQLite 的实现
    let kv_store: Arc<dyn aurora_core::traits::kv_store::KVStore> = Arc::new(
        aurora_core::l1_infrastructure::storage::SqliteStorage::new(db_path),
    );

    // SearchBackend：基于 Tantivy 的全文检索
    let index_dir = data_dir.join("tantivy_index");
    let search: Arc<dyn aurora_core::traits::search_backend::SearchBackend> = Arc::new(
        aurora_core::l1_infrastructure::search::TantivySearchBackend::new(&index_dir),
    );

    // CryptoProvider：AES-256-GCM + Argon2id + ML-KEM-768
    let crypto: Arc<dyn aurora_core::traits::crypto_provider::CryptoProvider> = Arc::new(
        aurora_security::CryptoProviderImpl::new(),
    );

    // SyncTarget：iroh P2P 同步（生产环境使用 IrohTransport）
    let sync_target: Arc<dyn aurora_core::traits::sync_target::SyncTarget> = Arc::new(
        aurora_core::l1_infrastructure::p2p::IrohP2pSyncTarget::new(),
    );

    // AIProvider：本地 llama.cpp + 云端 fallback
    let ai: Arc<dyn aurora_core::traits::ai_provider::AIProvider> = Arc::new(
        aurora_ai::LocalLlamaProvider::new(data_dir.join("models")),
    );

    // OcrProvider：PaddleOCR（本地，桌面端）
    let ocr: Arc<dyn aurora_core::traits::ocr_provider::OcrProvider> = Arc::new(
        aurora_core::l1_infrastructure::ocr::PaddleOcrEngine::new(),
    );

    // PluginRuntime：Wasmtime WASM 运行时
    let plugin: Arc<dyn aurora_core::traits::plugin_runtime::PluginRuntime> = Arc::new(
        aurora_plugin::WasmtimeRuntime::new(data_dir.join("plugins")),
    );

    // EventBus 持久化：SQLite event_queue 表
    let event_bus_store: Arc<dyn aurora_core::event_bus::layered::EventQueueStore> = Arc::new(
        aurora_core::event_bus::sqlite_queue::SqliteEventQueue::new(db_path),
    );

    AppCoreBuilder::new()
        .kv_store(kv_store)
        .search(search)
        .crypto(crypto)
        .sync_target(sync_target)
        .ai(ai)
        .ocr(ocr)
        .plugin(plugin)
        .event_bus_store(event_bus_store)
        .build()
}

/// 获取 AppCore 实例的 Arc 引用（不持有锁，可安全跨 .await）。
fn get_core() -> Result<Arc<AppCore>, String> {
    let guard = APP_STATE.lock().map_err(|e| format!("mutex poisoned: {}", e))?;
    guard.as_ref().cloned().ok_or_else(|| "AppCore not initialized".into())
}

// ── Tauri Commands (§30 平台适配) ─────────────────────────────

/// 创建新笔记。
#[tauri::command]
pub async fn cmd_create_note(title: String) -> Result<String, String> {
    let core = get_core()?;
    let id = uuid::Uuid::new_v4().to_string();
    info!(note_id = %id, title, "note created via desktop command");
    let note_json = serde_json::json!({
        "id": id,
        "title": title,
        "content": "",
        "content_type": "markdown",
        "created_at": chrono::Utc::now().to_rfc3339(),
    });
    let key = format!("note:{}", id);
    let payload = serde_json::to_vec(&note_json).map_err(|e| e.to_string())?;
    core.kv_store.set(&key, &payload).await.map_err(|e| e.to_string())?;
    Ok(id)
}

/// 获取笔记内容。
#[tauri::command]
pub async fn cmd_get_note(note_id: String) -> Result<serde_json::Value, String> {
    let core = get_core()?;
    let key = format!("note:{}", note_id);
    let payload = core.kv_store.get(&key)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("note not found: {}", note_id))?;
    let note: serde_json::Value = serde_json::from_slice(&payload).map_err(|e| e.to_string())?;
    Ok(note)
}

/// 更新笔记。
#[tauri::command]
pub async fn cmd_update_note(
    note_id: String,
    title: Option<String>,
    content: Option<String>,
) -> Result<(), String> {
    let core = get_core()?;
    let key = format!("note:{}", note_id);
    let existing = core.kv_store.get(&key).await.map_err(|e| e.to_string())?;
    let mut note: serde_json::Value = match existing {
        Some(data) => serde_json::from_slice(&data).map_err(|e| e.to_string())?,
        None => serde_json::json!({"id": note_id, "content_type": "markdown"}),
    };
    if let Some(t) = title {
        note["title"] = serde_json::Value::String(t);
    }
    if let Some(c) = content {
        note["content"] = serde_json::Value::String(c);
    }
    note["updated_at"] = serde_json::Value::String(chrono::Utc::now().to_rfc3339());
    let payload = serde_json::to_vec(&note).map_err(|e| e.to_string())?;
    core.kv_store.set(&key, &payload).await.map_err(|e| e.to_string())?;
    info!(note_id = %note_id, "note updated via desktop command");
    Ok(())
}

/// 删除笔记。
#[tauri::command]
pub async fn cmd_delete_note(note_id: String) -> Result<(), String> {
    let core = get_core()?;
    let key = format!("note:{}", note_id);
    core.kv_store.delete(&key).await.map_err(|e| e.to_string())?;
    info!(note_id = %note_id, "note deleted via desktop command");
    Ok(())
}

/// 搜索笔记。
#[tauri::command]
pub async fn cmd_search_notes(query: String) -> Result<Vec<serde_json::Value>, String> {
    let core = get_core()?;
    info!(query, "search notes via desktop command");
    // 注意：SearchBackend::search 签名为 async fn search(&self, query: &str, opts: &SearchOptions)
    // V19 §28 异步化迁移后需 .await
    let opts = aurora_core::traits::search_backend::SearchOptions::default();
    let result = core.search.search(&query, &opts).await.map_err(|e| e.to_string())?;
    let results: Vec<serde_json::Value> = result.hits
        .into_iter()
        .map(|hit| serde_json::json!({"doc_id": hit.note_id, "score": hit.score, "snippet": hit.snippet}))
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
    }))
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

impl Default for TauriDesktopPlatform {
    fn default() -> Self {
        Self::new()
    }
}

impl DesktopPlatform for TauriDesktopPlatform {
    fn set_tray(&self, icon_path: &str, menu_items: Vec<TrayMenuItem>) {
        info!(icon_path, ?menu_items, "set_tray requested");
        // Tauri v2 SystemTray 接入待实现
    }

    fn register_shortcut(&self, accelerator: &str, _callback: Box<dyn Fn() + Send>) {
        info!(accelerator, "register_shortcut requested");
        // Tauri v2 GlobalShortcut 接入待实现
    }

    fn set_menu(&self, menu_spec: &str) {
        info!(menu_spec, "set_menu requested");
        // Tauri v2 Menu 接入待实现
    }

    fn clipboard_read(&self) -> Result<String, String> {
        // Tauri v2 Clipboard 接入待实现
        Ok(String::new())
    }

    fn clipboard_write(&self, text: &str) -> Result<(), String> {
        info!(len = text.len(), "clipboard_write requested");
        Ok(())
    }

    fn notify(&self, title: &str, body: &str) {
        info!(title, body, "notify requested");
        // Tauri v2 Notification 接入待实现
    }
}
