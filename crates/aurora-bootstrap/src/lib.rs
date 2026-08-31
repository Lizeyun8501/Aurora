//! Aurora Note 应用启动装配（V19 §36.1 共享流程）
//!
//! 桌面（Tauri）与移动（UniFFI）平台适配层共用的 AppCore 依赖注入装配，
//! 保证各平台启动流程一致、避免装配代码漂移：
//!
//! 1. 创建数据目录；
//! 2. 打开 SQLite 数据库并执行迁移（`aurora-migration`）；
//! 3. 加载/创建本地 DEK 保险库（E2EE：明文 → AES-256-GCM → 密文落库）；
//! 4. 构造 7 大 Trait 默认实现（LoroCrdtEngine / IrohSyncTarget / ...）并注入
//!    [`AppCoreBuilder`]（V19 §36.1 步骤 4-5）；
//! 5. `app_core.startup()` → 重放未消费事件 + 健康检查（ARCH-003）。
//!
//! # 平台差异
//! 平台层仍需自行决定数据目录（桌面用 `dirs_next::data_dir()`，移动端用
//! 沙盒目录），以及平台专属能力（托盘/快捷键/通知等）的注册。

use std::path::Path;
use std::sync::Arc;

use aurora_core::app_core::{AppCore, AppCoreBuilder};
use aurora_core::l3_domain::system_settings::SystemSettings;
use aurora_security::LocalDekVault;
use tracing::info;

/// 启动装配结果：平台层持有 core 与 vault 供 command/FFI 复用。
pub struct BootedApp {
    /// 已启动的应用核心（含 7 大 Trait + 事件总线）。
    pub core: Arc<AppCore>,
    /// 本地 DEK 保险库（笔记加解密密钥）。
    pub vault: Arc<LocalDekVault>,
}

/// 启动装配错误。
#[derive(Debug, thiserror::Error)]
pub enum BootstrapError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("database migration failed: {0}")]
    Migration(#[from] aurora_migration::MigrationError),
    #[error("vault initialization failed: {0}")]
    Vault(#[from] aurora_security::Error),
    #[error("core initialization failed: {0}")]
    Core(String),
}

impl From<aurora_core::Error> for BootstrapError {
    fn from(e: aurora_core::Error) -> Self {
        Self::Core(e.to_string())
    }
}

/// 执行完整启动装配（V19 §36.1）。
///
/// # Errors
/// 迁移、DEK 初始化、Trait 构造或 `startup()` 任一步失败均返回
/// [`BootstrapError`]（启动期早失败）。
pub fn bootstrap(data_dir: &Path) -> Result<BootedApp, BootstrapError> {
    std::fs::create_dir_all(data_dir)?;

    // 打开 SQLite 数据库并执行迁移
    let db_path = data_dir.join("aurora.db");
    let migration = aurora_migration::MigrationManager::new(&db_path)?;
    migration.migrate()?;
    info!(db_path = ?db_path, "SQLite migrations complete");

    // E2EE：加载/创建本地 DEK 保险库（V19 §10：明文 → 密文落库）
    let vault = Arc::new(LocalDekVault::load_or_create(&data_dir.join("keys"))?);

    // 构造各 Trait 默认实现并注入 AppCoreBuilder
    let core = Arc::new(build_app_core(data_dir, &db_path)?);
    core.startup()?;
    info!(data_dir = ?data_dir, "AppCore startup complete");

    Ok(BootedApp { core, vault })
}

/// 构造 AppCore 并注入各 Trait 默认实现（V19 §36.1 步骤 4-5）。
fn build_app_core(data_dir: &Path, db_path: &Path) -> Result<AppCore, BootstrapError> {
    // 加载系统设置（V19 §16）：本轮先用默认值，后续 PR 可改为从 SQLite 的
    // settings_layer 表读出已持久化的 SystemSettings。
    let core_settings = SystemSettings::new();

    // KVStore：基于 SQLite 的实现
    let kv_store: Arc<dyn aurora_core::traits::kv_store::KVStore> = Arc::new(
        aurora_core::l1_infrastructure::storage::SqliteStorage::new(db_path)?,
    );

    // SearchBackend：基于 Tantivy 的全文检索（V19 §28.7）
    let index_dir = data_dir.join("tantivy_index");
    let search: Arc<dyn aurora_core::traits::search_backend::SearchBackend> =
        Arc::new(aurora_core::l1_infrastructure::search::TantivySearchBackend::new(&index_dir)?);

    // CryptoProvider：AES-256-GCM + Argon2id + ML-KEM-768（V19 §28.6 注入式实现）
    let crypto: Arc<dyn aurora_core::traits::crypto_provider::CryptoProvider> =
        Arc::new(aurora_security::crypto_provider_impl::SecurityCryptoProvider::new());

    // SyncTarget：iroh P2P 同步
    let sync_target: Arc<dyn aurora_core::traits::sync_target::SyncTarget> =
        Arc::new(aurora_core::l1_infrastructure::p2p::IrohSyncTarget::new());

    // AIProvider：本地 Ollama HTTP（V19 §7.2「本地 AI」），本地不可达时降级到
    // 可选的云 Provider（OpenAI 兼容端点）。云端 fallback 仅在用户通过
    // SystemSettings.ai.cloud_api_key 配置之后才会启用（默认 None）。
    let ai_settings = &core_settings.ai;
    let cloud: Option<Arc<dyn aurora_core::traits::ai_provider::AIProvider>> =
        if ai_settings.cloud_configured() {
            Some(Arc::new(aurora_ai::OpenAiCompatProvider::new(
                ai_settings.cloud_base_url.clone().unwrap_or_default(),
                ai_settings.cloud_api_key.clone().unwrap_or_default(),
                ai_settings
                    .cloud_model
                    .clone()
                    .unwrap_or_else(|| "gpt-4o-mini".to_string()),
            )))
        } else {
            None
        };
    let ai: Arc<dyn aurora_core::traits::ai_provider::AIProvider> = {
        let provider = aurora_ai::OllamaProvider::new_with_fallback(
            ai_settings.ollama_base_url.clone(),
            ai_settings.ollama_model.clone(),
            cloud,
        );
        // 后台周期探测 is_available；无 tokio runtime 时自动跳过探测。
        provider.start_probing();
        Arc::new(provider)
    };

    // OcrProvider：PaddleOCR（本地）
    let ocr: Arc<dyn aurora_core::traits::ocr_provider::OcrProvider> =
        Arc::new(aurora_core::l1_infrastructure::ocr::PaddleOcrEngine::new());

    // PluginRuntime：Wasmtime WASM 运行时（V19 §28.5）
    let plugin: Arc<dyn aurora_core::traits::plugin_runtime::PluginRuntime> =
        Arc::new(aurora_core::l1_infrastructure::wasm::WasmtimeRuntime::new()?);

    // EventBus 持久化：SQLite event_queue 表（ARCH-003）
    let event_bus_store: Arc<dyn aurora_core::event_bus::layered::EventQueueStore> = Arc::new(
        aurora_core::event_bus::sqlite_queue::SqliteEventQueue::new(db_path)?,
    );

    // V20 Phase 1 §4.5: 搜索索引投影（读模型）— 消费事件投影到 Tantivy，
    // 水位线存 KVStore；启动 catch_up 增量追赶，verify 失败自动全量重建。
    // 全量数据源：暂时为空（笔记主存储接入 NoteDoc 后由存储层提供回调）。
    let search_projection: Arc<aurora_core::l2_engines::search_projection::SearchIndexProjection> =
        Arc::new(aurora_core::l2_engines::search_projection::SearchIndexProjection::new(
            search.clone(),
            kv_store.clone(),
            Box::new(Vec::new),
        ));

    Ok(AppCoreBuilder::new()
        .kv_store(kv_store)
        .search(search)
        .crypto(crypto)
        .sync_target(sync_target)
        .ai(ai)
        .ocr(ocr)
        .plugin(plugin)
        .event_bus_store(event_bus_store)
        .projection(search_projection)
        .build())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_creates_runnable_core() {
        let dir = tempfile::tempdir().unwrap();
        let app = bootstrap(dir.path()).unwrap();
        assert_eq!(app.core.crypto.algorithm_version(), 1);
        assert!(dir.path().join("aurora.db").exists());
        assert!(dir.path().join("keys").join("dek.bin").exists());
    }

    /// V20 Phase 1 退出条件: 杀进程后索引自动补齐（投影水位线增量追赶）。
    #[tokio::test]
    async fn bootstrap_projection_catch_up_across_restart() {
        let dir = tempfile::tempdir().unwrap();

        // ── 第一次启动: 发事件 → 投影追赶 ──
        {
            let app = bootstrap(dir.path()).unwrap();
            app.core.startup().unwrap();
            app.core.event_bus.publish(
                aurora_core::event_bus::layered::AppEvent::NoteCreated {
                    note_id: "n1".into(),
                    title: "projection verify note".into(),
                    content: "V20 Phase 1".into(),
                },
            );
            app.core.catch_up_projections().await.unwrap();

            let hits = app
                .core
                .search
                .search("projection", &aurora_core::traits::search_backend::SearchOptions::default())
                .await
                .unwrap();
            assert_eq!(hits.hits.len(), 1, "事件应已投影到索引");
        } // core drop = 模拟杀进程

        // ── 第二次启动: seq 恢复 + 新事件 + 增量追赶 ──
        {
            let app2 = bootstrap(dir.path()).unwrap();
            app2.core.startup().unwrap(); // restore_seq + replay
            app2.core.event_bus.publish(
                aurora_core::event_bus::layered::AppEvent::NoteCreated {
                    note_id: "n2".into(),
                    title: "after restart".into(),
                    content: String::new(),
                },
            );
            app2.core.catch_up_projections().await.unwrap();

            // 分开查询两篇（空查询在 QueryParser 下不可靠）
            let old = app2
                .core
                .search
                .search("projection", &aurora_core::traits::search_backend::SearchOptions::default())
                .await
                .unwrap();
            let ids: Vec<&str> = old.hits.iter().map(|h| h.note_id.as_str()).collect();
            assert!(ids.contains(&"n1"), "旧索引跨重启保留: {ids:?}");

            let new = app2
                .core
                .search
                .search("restart", &aurora_core::traits::search_backend::SearchOptions::default())
                .await
                .unwrap();
            let ids2: Vec<&str> = new.hits.iter().map(|h| h.note_id.as_str()).collect();
            assert!(ids2.contains(&"n2"), "新事件增量投影: {ids2:?}");
        }
    }

    #[test]
    fn bootstrap_is_idempotent_across_restarts() {
        let dir = tempfile::tempdir().unwrap();
        let first = bootstrap(dir.path()).unwrap();
        let second = bootstrap(dir.path()).unwrap();
        assert_eq!(
            first.vault.dek(),
            second.vault.dek(),
            "restart must reuse DEK"
        );
    }
}
