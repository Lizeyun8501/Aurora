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
use tauri::Manager;

use aurora_core::app_core::AppCore;
use aurora_core::l2_engines::hybrid_search::HybridSearcher;
use aurora_core::l2_engines::vector_search::{vector_index_for, EmbedProvider, VectorIndex};
use aurora_security::LocalDekVault;
use tracing::{info, warn};

use aurora_ai::embed::EmbedFromAiProvider;

// ── 启动期常量与全局状态 ─────────────────────────────

mod ai_commands;
mod attachment_commands;
mod backup_commands;
mod import_commands;
mod sync_commands;

/// 桌面端默认数据目录名。
const AURORA_DIR_NAME: &str = "aurora";
/// 默认工作区 ID（单工作区模式；多工作区接入后改为按用户选择注入）。
const DEFAULT_WORKSPACE_ID: &str = "default";

/// 桌面端应用核心（全局单例）。
static APP_STATE: Mutex<Option<Arc<AppCore>>> = Mutex::new(None);
/// 本地 DEK 保险库（全局单例）。
static VAULT_STATE: Mutex<Option<Arc<LocalDekVault>>> = Mutex::new(None);
/// 附件存储（DK-09 — bootstrap 装配，setup 注入）。
static ATTACH_STATE: Mutex<Option<Arc<dyn aurora_core::attachment_store::AttachmentStore>>> =
    Mutex::new(None);
/// 同步门（DK-08 §7.3 — bootstrap 装配，setup 注入；wifi_only 运行时切换）。
/// 注：读取侧暂无调用方（wifi_only 读写经 BootedApp 同源语义），访问器随需再补。
static SYNC_GATE_STATE: Mutex<Option<Arc<aurora_sync::sync_gate::SyncGate>>> = Mutex::new(None);
/// 向量索引（DK-03 S3b — 混合检索向量路 + 索引管线写入共用）。
static VEC_STATE: Mutex<Option<Arc<VectorIndex>>> = Mutex::new(None);
/// 嵌入供给方（S3b — 索引管线与回填共用，从 HybridSearcher 不可逆取故独立存）。
static EMBED_STATE: Mutex<Option<Arc<dyn EmbedProvider>>> = Mutex::new(None);
/// 混合检索器（S3b — BM25+KNN→RRF，cmd_search_notes hybrid 模式走此路）。
static HYBRID_STATE: Mutex<Option<Arc<HybridSearcher>>> = Mutex::new(None);

/// 获取向量索引引用（S3b）。
fn get_vectors() -> Result<Arc<VectorIndex>, String> {
    VEC_STATE
        .lock()
        .map_err(|e| format!("mutex poisoned: {}", e))?
        .as_ref()
        .cloned()
        .ok_or_else(|| "vector index not initialized".into())
}

/// 获取嵌入供给方引用（S3b）。
fn get_embed() -> Result<Arc<dyn EmbedProvider>, String> {
    EMBED_STATE
        .lock()
        .map_err(|e| format!("mutex poisoned: {}", e))?
        .as_ref()
        .cloned()
        .ok_or_else(|| "embed provider not initialized".into())
}

/// 获取混合检索器引用（S3b）。
fn get_hybrid() -> Result<Arc<HybridSearcher>, String> {
    HYBRID_STATE
        .lock()
        .map_err(|e| format!("mutex poisoned: {}", e))?
        .as_ref()
        .cloned()
        .ok_or_else(|| "hybrid searcher not initialized".into())
}

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

/// DK-20：笔记内容加密对（NoteContentCipher HKDF 每笔记密钥——复用 DK-07 S2
/// aurora-security 实现，装配进 WriteContext.content_cipher）。桌面端 TODO(DK-07) 关闭。
fn content_cipher_pair(
) -> Result<std::sync::Arc<aurora_core::write_path::ContentCipherPair>, String> {
    let vault = get_vault()?;
    let dek =
        aurora_security::key_hierarchy::WorkspaceDek::from_bytes("vault-master", *vault.dek());
    let enc = aurora_security::note_cipher::NoteContentCipher::new(dek.clone());
    let dec = aurora_security::note_cipher::NoteContentCipher::new(dek);
    Ok(std::sync::Arc::new(
        aurora_core::write_path::ContentCipherPair {
            encrypt: Box::new(move |note_id: &str, plain: &str| enc.encrypt(note_id, plain)),
            decrypt: Box::new(move |note_id: &str, sealed: &str| dec.decrypt(note_id, sealed)),
        },
    ))
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
            cmd_get_note_snapshot,
            cmd_save_note_snapshot,
            cmd_get_note,
            cmd_update_note,
            cmd_delete_note,
            cmd_list_trashed,
            cmd_create_folder,
            cmd_move_note,
            cmd_rename_folder,
            cmd_list_tree,
            cmd_smartfolder_create,
            cmd_daily_note_open,
            cmd_daily_note_set_mode,
            cmd_daily_note_get_mode,
            cmd_daily_note_set_template,
            cmd_daily_note_get_template,
            cmd_smartfolder_set_rule,
            cmd_smartfolder_get_rule,
            cmd_smartfolder_list_items,
            cmd_restore_note,
            cmd_purge_note,
            cmd_purge_expired_trash,
            cmd_search_notes,
            cmd_app_status,
            cmd_today_view_stats,
            cmd_set_task_dependency,
            cmd_remove_task_dependency,
            cmd_blocked_task_ids,
            cmd_today_task_rows,
            cmd_get_task_dependencies,
            cmd_pomodoro_state,
            cmd_pomodoro_start,
            cmd_pomodoro_pause,
            cmd_pomodoro_reset,
            cmd_pomodoro_config,
            cmd_pomodoro_sync,
            cmd_weekly_review,
            cmd_get_backlinks,
            cmd_due_review_cards,
            cmd_review_card,
            attachment_commands::cmd_read_attachment,
            ai_commands::cmd_get_ai_cloud_policy,
            backup_commands::cmd_backup_now,
            backup_commands::cmd_backup_status,
            ai_commands::cmd_set_ai_cloud_policy,
            ai_commands::cmd_ai_liquify_proposal,
            ai_commands::cmd_ai_list_liquify_proposals,
            ai_commands::cmd_ai_commit_liquify,
            ai_commands::cmd_ai_reject_liquify_proposal,
            ai_commands::cmd_agent_session_create,
            ai_commands::cmd_agent_session_kill,
            ai_commands::cmd_agent_session_status,
            ai_commands::cmd_agent_audit_recent,
            ai_commands::cmd_agent_audit_note,
            sync_commands::cmd_get_wifi_only,
            sync_commands::cmd_set_wifi_only,
            import_commands::cmd_plan_import,
            import_commands::cmd_import_markdown_dir,
            import_commands::cmd_import_enex,
            import_commands::cmd_import_opml,
        ])
        .setup(|app| {
            tracing_subscriber::fmt::init();
            info!("Aurora Desktop starting");

            let data_dir = get_data_dir();
            info!(data_dir = ?data_dir, "data directory ready");

            // 共享装配：迁移 + DEK 保险库 + AppCore DI 注入 + startup（V19 §36.1）
            let booted = aurora_bootstrap::bootstrap(
                &data_dir,
                std::sync::Arc::new(aurora_sync::sync_gate::AlwaysUnmetered),
            )
            .map_err(box_err)?;
            *VAULT_STATE.lock().expect("VAULT_STATE mutex poisoned") = Some(booted.vault.clone());
            *SYNC_GATE_STATE
                .lock()
                .expect("SYNC_GATE_STATE mutex poisoned") = Some(booted.sync_gate.clone());
            *APP_STATE.lock().expect("APP_STATE mutex poisoned") = Some(booted.core.clone());
            *ATTACH_STATE.lock().expect("ATTACH_STATE mutex poisoned") =
                Some(booted.attachments.clone());
            // BOOTED_STATE 的 move 必须最后——booted 的其余字段先 clone 完
            // （E0382 borrow-after-move，49ae86c 引入 · 2026-09-25 修）。
            let booted_arc = std::sync::Arc::new(booted);
            *sync_commands::BOOTED_STATE
                .lock()
                .expect("BOOTED_STATE mutex poisoned") = Some(booted_arc.clone());
            info!("AppCore startup complete");

            // DK-03 S3b：混合检索装配（向量路 768 = nomic-embed-text 本地口径）。
            // 诚实标注：维度按配置口径冻结，AIProvider 实际模型维度不符时
            // index_note 维度守卫报错 → 索引管线 log warn 静默失效，BM25 兜底——
            // 不做 probe 惰性组装（模型/维度配置化留待配置卡）。
            {
                let core = booted_arc.core.clone();
                let vec_idx = Arc::new(vector_index_for(&core, "nomic-embed-text", 768));
                let embed: Arc<dyn EmbedProvider> = Arc::new(EmbedFromAiProvider::new(
                    core.ai.clone(),
                    "nomic-embed-text",
                    768,
                ));
                let hybrid = Arc::new(HybridSearcher::new(
                    core.search.clone(),
                    vec_idx.clone(),
                    embed.clone(),
                ));
                *VEC_STATE.lock().expect("VEC_STATE mutex poisoned") = Some(vec_idx);
                *EMBED_STATE.lock().expect("EMBED_STATE mutex poisoned") = Some(embed.clone());
                *HYBRID_STATE.lock().expect("HYBRID_STATE mutex poisoned") = Some(hybrid);

                // boot 后向量回填（限速 250ms/篇防本地推理过载；content_of 为同步
                // 闭包——预载解密内容表是唯一路径，个人库量级可控）。
                let core_for_backfill = core.clone();
                tauri::async_runtime::spawn(async move {
                    let Ok(vault) = get_vault() else { return };
                    let Ok(pairs) = core_for_backfill.kv_store.scan_prefix("note:").await else {
                        warn!("vector backfill: note scan failed");
                        return;
                    };
                    let mut contents: std::collections::HashMap<String, String> =
                        std::collections::HashMap::new();
                    for (k, v) in pairs {
                        let Some(id) = k.strip_prefix("note:").map(String::from) else {
                            continue;
                        };
                        match unwrap_note_bytes(&core_for_backfill, &vault, &v) {
                            Ok(json) => {
                                if let Some(c) = json.get("content").and_then(|x| x.as_str()) {
                                    contents.insert(id, c.to_string());
                                }
                            }
                            Err(e) => warn!(note_id = %id, error = %e, "backfill: note decrypt failed"),
                        }
                    }
                    let Ok(vecs) = get_vectors() else { return };
                    let Ok(embed_ref) = get_embed() else { return };
                    let content_of = |id: &str| contents.get(id).cloned();
                    match vecs
                        .backfill_missing(&content_of, embed_ref.as_ref(), 50, std::time::Duration::from_millis(250))
                        .await
                    {
                        Ok(n) if n > 0 => info!(backfilled = n, "vector backfill done"),
                        Ok(_) => info!("vector backfill: nothing to do"),
                        Err(e) => warn!(error = %e, "vector backfill failed (embedding unavailable? BM25 fallback active)"),
                    }
                });
            }

            // DK-17 S2：运行时定时器（S1 挂起项收敛）——每小时 tick 备份水位 +
            // 每周完整性校验（check_and_backup 内含水 位/校验判断，幂等）。
            {
                let booted_for_tick = booted_arc.clone();
                tauri::async_runtime::spawn(async move {
                    let mut ticker = tokio::time::interval(std::time::Duration::from_secs(3600));
                    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                    ticker.tick().await; // 首跳立即返回——boot 已做过水位备份/校验，跳过
                    loop {
                        ticker.tick().await;
                        match booted_for_tick.maybe_backup_on_boot() {
                            Ok(Some(r)) => info!(path = ?r.path, "hourly tick backup created"),
                            Ok(None) => {}
                            Err(e) => warn!(error = %e, "hourly tick backup failed"),
                        }
                        match booted_for_tick.verify_backups_if_due() {
                            Ok(Some(true)) => info!("hourly tick weekly verify passed"),
                            Ok(Some(false)) => warn!("hourly tick weekly verify FAILED"),
                            Ok(None) => {}
                            Err(e) => warn!(error = %e, "hourly tick verify errored"),
                        }
                    }
                });
            }

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

// ── V26 I2/DK-01W: WritePath 唯一写入入口 ─────────────────────

/// blocks 双轨存储（进程级单例；复用 migration 建的 aurora.db；打开失败 = 内存降级）。
fn blocks_store() -> Option<std::sync::Arc<aurora_core::blocks::BlockStore>> {
    static BLOCKS: std::sync::OnceLock<Option<std::sync::Arc<aurora_core::blocks::BlockStore>>> =
        std::sync::OnceLock::new();
    BLOCKS
        .get_or_init(|| {
            let dir = dirs_next::data_dir()
                .map(|d| d.join("aurora-note"))
                .unwrap_or_else(|| std::path::PathBuf::from("."));
            aurora_core::blocks::BlockStore::open(&dir.join("aurora.db")).map(std::sync::Arc::new)
        })
        .clone()
}

// ── Tauri Commands（§30 平台适配） ─────────────────────

/// 创建新笔记（加密存储 + 建立搜索索引）。
#[tauri::command]
async fn cmd_create_note(title: String) -> Result<String, String> {
    let core = get_core()?;
    let vault = get_vault()?;
    let blocks = blocks_store();
    // V26 I2/DK-01W: 唯一写入入口（闭包与 ctx 同栈，借用链一致）
    // V26 DK-01W: owned ctx（'static boxed 闭包，避免 async 自借用）
    let crypto = core.crypto.clone();
    let vault_seal = vault.clone();
    let seal = move |b: &[u8]| {
        vault_seal
            .encrypt(crypto.as_ref(), b)
            .map_err(|e| aurora_core::Error::Internal(e.to_string()))
    };
    let crypto2 = core.crypto.clone();
    let vault_unseal = vault.clone();
    let unseal = move |b: &[u8]| {
        vault_unseal
            .decrypt(crypto2.as_ref(), b)
            .map_err(|e| aurora_core::Error::Internal(e.to_string()))
    };
    let ctx = aurora_core::write_path::WriteContext {
        core: core.clone(),
        blocks,
        seal: Some(aurora_core::write_path::SealPair {
            seal: Box::new(seal),
            unseal: Box::new(unseal),
        }),
        content_cipher: Some(content_cipher_pair()?),
        attachments: ATTACH_STATE
            .lock()
            .expect("ATTACH_STATE mutex poisoned")
            .clone(), // DK-09
    };
    let id = aurora_core::write_path::create_note(&ctx, &title)
        .await
        .map_err(|e| e.to_string())?;
    // 投影事件驱动（搜索/双链/任务 — TodayView 桌面端从此有数据）
    core.catch_up_projections()
        .await
        .map_err(|e| e.to_string())?;
    info!(note_id = %id, "note created via desktop WritePath");
    Ok(id)
}

/// 获取笔记内容（解密后返回）。
#[tauri::command]
async fn cmd_get_note(note_id: String) -> Result<serde_json::Value, String> {
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

/// 编辑器快照读取（DK-05 S3）：kv `notesnap:{id}` 全量快照（含前端
/// loro-prosemirror "doc" 容器 + 内核容器）。无快照返回 null（前端降级 md 灌入）。
#[tauri::command]
async fn cmd_get_note_snapshot(note_id: String) -> Result<Option<String>, String> {
    use base64::Engine;
    let core = get_core()?;
    let bytes = core
        .kv_store
        .get(&format!("notesnap:{note_id}"))
        .await
        .map_err(|e| e.to_string())?;
    Ok(bytes
        .filter(|b| !b.is_empty())
        .map(|b| base64::engine::general_purpose::STANDARD.encode(b)))
}

/// 编辑器快照写入（DK-05 S3）：CRDT 合并语义（mobile save_note_snapshot_impl
/// 同款）—— apply_update import 合并非替换，内核容器与编辑器容器共存不互覆，
/// P2P 对端修改不丢。空快照视为无操作。
#[tauri::command]
async fn cmd_save_note_snapshot(note_id: String, snapshot_b64: String) -> Result<(), String> {
    use base64::Engine;
    let core = get_core()?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&snapshot_b64)
        .map_err(|e| e.to_string())?;
    if bytes.is_empty() {
        return Ok(());
    }
    let merged = match core.kv_store.get(&format!("notesnap:{note_id}")).await {
        Ok(Some(existing)) if !existing.is_empty() => {
            let doc = aurora_core::l1_infrastructure::note_doc::NoteDoc::from_snapshot(&existing)
                .map_err(|e| e.to_string())?;
            doc.apply_update(&bytes).map_err(|e| e.to_string())?;
            doc.export_snapshot().map_err(|e| e.to_string())?
        }
        _ => bytes,
    };
    core.kv_store
        .set(&format!("notesnap:{note_id}"), &merged)
        .await
        .map_err(|e| e.to_string())
}

/// 更新笔记（解密 → 修改 → 重新加密落库 + 更新索引）。
#[tauri::command]
async fn cmd_update_note(
    note_id: String,
    title: Option<String>,
    content: Option<String>,
) -> Result<(), String> {
    let core = get_core()?;
    let vault = get_vault()?;
    let blocks = blocks_store();
    // V26 I2/DK-01W: 走 WritePath（含 Loro 快照 + blocks 派生 + 事件）
    // V26 DK-01W: owned ctx（'static boxed 闭包，避免 async 自借用）
    let crypto = core.crypto.clone();
    let vault_seal = vault.clone();
    let seal = move |b: &[u8]| {
        vault_seal
            .encrypt(crypto.as_ref(), b)
            .map_err(|e| aurora_core::Error::Internal(e.to_string()))
    };
    let crypto2 = core.crypto.clone();
    let vault_unseal = vault.clone();
    let unseal = move |b: &[u8]| {
        vault_unseal
            .decrypt(crypto2.as_ref(), b)
            .map_err(|e| aurora_core::Error::Internal(e.to_string()))
    };
    let ctx = aurora_core::write_path::WriteContext {
        core: core.clone(),
        blocks,
        seal: Some(aurora_core::write_path::SealPair {
            seal: Box::new(seal),
            unseal: Box::new(unseal),
        }),
        content_cipher: Some(content_cipher_pair()?),
        attachments: ATTACH_STATE
            .lock()
            .expect("ATTACH_STATE mutex poisoned")
            .clone(), // DK-09
    };
    if let Some(t) = title.as_deref() {
        if title.is_some() {
            aurora_core::write_path::rename_note(&ctx, &note_id, t)
                .await
                .map_err(|e| e.to_string())?;
        }
    }
    if let Some(c) = content.as_deref() {
        aurora_core::write_path::save_note_content(&ctx, &note_id, c)
            .await
            .map_err(|e| e.to_string())?;
        // DK-03 S3b：向量索引管线（fire-and-forget——Ollama 缺席时静默失效，
        // BM25 兜底；hash 去重由 index_note 内建，重复保存不重嵌）。
        if !c.is_empty() {
            if let (Ok(vecs), Ok(embed)) = (get_vectors(), get_embed()) {
                let note_id2 = note_id.clone();
                let content2 = c.to_string();
                tauri::async_runtime::spawn(async move {
                    match vecs.index_note(&note_id2, &content2, embed.as_ref()).await {
                        Ok(outcome) => {
                            info!(note_id = %note_id2, outcome = ?outcome, "vector indexed")
                        }
                        Err(e) => warn!(note_id = %note_id2, error = %e, "vector index failed"),
                    }
                });
            }
        }
    }
    core.catch_up_projections()
        .await
        .map_err(|e| e.to_string())?;
    info!(note_id = %note_id, "note updated via desktop WritePath");
    Ok(())
}

// ── V23-I1: 双端能力对齐（今日视图/反链/FSRS 复习） ──────────

/// 今日视图统计（移动端 today_view_stats 同源 — AppCore 任务投影聚合）。
#[tauri::command]
async fn cmd_today_view_stats() -> Result<serde_json::Value, String> {
    let core = get_core()?;
    let (active, done) = core.task_projection_stats();
    let due_today = core.task_projection_due_today();
    Ok(serde_json::json!({
        "active": active,
        "done": done,
        "due_today": due_today,
    }))
}

// === DK-21 S2：任务依赖命令面（blocked 徽章数据源）===

type ProjRef<'a> = &'a aurora_core::l2_engines::task_projection::TaskProjection;

/// 从 AppCore 投影注册表取任务投影（借用绑定 core 局部 Arc，跨 await 安全）。
fn task_proj<'a>(core: &'a std::sync::Arc<AppCore>) -> Result<ProjRef<'a>, String> {
    core.projections()
        .iter()
        .find_map(|p| {
            p.as_any().and_then(|a| {
                a.downcast_ref::<aurora_core::l2_engines::task_projection::TaskProjection>()
            })
        })
        .ok_or_else(|| "task projection not registered".into())
}

/// 设置依赖边（成环/自依赖拒绝——环路径可读返回 Err）。
#[tauri::command]
async fn cmd_set_task_dependency(task_id: String, depends_on: String) -> Result<(), String> {
    let core = get_core()?;
    task_proj(&core)?
        .set_dependency(&task_id, &depends_on)
        .await
}

/// 移除依赖边。
#[tauri::command]
async fn cmd_remove_task_dependency(task_id: String, depends_on: String) -> Result<(), String> {
    let core = get_core()?;
    task_proj(&core)?
        .remove_dependency(&task_id, &depends_on)
        .await
}

/// blocked 任务 id 列表（派生态实时计算——前置非终态即阻塞）。
#[tauri::command]
async fn cmd_blocked_task_ids() -> Result<Vec<String>, String> {
    let core = get_core()?;
    let mut ids = task_proj(&core)?
        .blocked_ids()
        .await
        .into_iter()
        .collect::<Vec<_>>();
    ids.sort();
    Ok(ids)
}

/// 读取任务依赖边（前置列表）。
#[tauri::command]
async fn cmd_get_task_dependencies(task_id: String) -> Result<Vec<String>, String> {
    let core = get_core()?;
    Ok(task_proj(&core)?.get_dependencies(&task_id).await)
}

/// 今日任务行 + blocked 标记（TodayView 列表渲染数据面）。
#[tauri::command]
async fn cmd_today_task_rows() -> Result<Vec<serde_json::Value>, String> {
    let core = get_core()?;
    let proj = task_proj(&core)?;
    let now_ms = chrono::Utc::now().timestamp_millis();
    let blocked = proj.blocked_ids().await;
    let mut rows = proj
        .today(now_ms)
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "task_id": r.task_id,
                "title": r.title,
                "status": r.status,
                "priority": r.priority,
                "due_date": r.due_date,
                "blocked": blocked.contains(&r.task_id),
            })
        })
        .collect::<Vec<_>>();
    // blocked 优先展示（阻塞任务前置——用户先处理依赖）
    rows.sort_by(|a, b| {
        let ba = a["blocked"].as_bool().unwrap_or(false);
        let bb = b["blocked"].as_bool().unwrap_or(false);
        bb.cmp(&ba)
            .then(b["task_id"].as_str().cmp(&a["task_id"].as_str()))
    });
    Ok(rows)
}

// === DK-22 S2：番茄钟 / 周回顾命令面 ===

/// 全局番茄钟实例（desktop 层持有 — core `PomodoroTimer` 线程安全，
/// 无持久化需求：计时态生命周期 = 应用会话）。
static POMODORO: std::sync::OnceLock<aurora_core::l3_domain::today_view::PomodoroTimer> =
    std::sync::OnceLock::new();

fn pomodoro() -> &'static aurora_core::l3_domain::today_view::PomodoroTimer {
    POMODORO.get_or_init(aurora_core::l3_domain::today_view::PomodoroTimer::default)
}

fn pomo_state_json() -> serde_json::Value {
    let s = pomodoro().state();
    serde_json::json!({
        "phase": s.phase,
        "remaining_seconds": s.remaining_seconds,
        "completed_work_cycles": s.completed_work_cycles,
        "running": s.running,
    })
}

/// 番茄钟当前状态（前端 1s 轮询渲染）。
#[tauri::command]
async fn cmd_pomodoro_state() -> Result<serde_json::Value, String> {
    Ok(pomo_state_json())
}

/// 启动 / 暂停后恢复（重锚挂钟）。
#[tauri::command]
async fn cmd_pomodoro_start() -> Result<serde_json::Value, String> {
    pomodoro().start();
    Ok(pomo_state_json())
}

/// 暂停（计时冻结）。
#[tauri::command]
async fn cmd_pomodoro_pause() -> Result<serde_json::Value, String> {
    pomodoro().pause();
    Ok(pomo_state_json())
}

/// 重置回工作相位满格。
#[tauri::command]
async fn cmd_pomodoro_reset() -> Result<serde_json::Value, String> {
    pomodoro().reset();
    Ok(pomo_state_json())
}

/// 配置工作/休息时长（分钟）。会重置当前计时（配置变更语义）。
#[tauri::command]
async fn cmd_pomodoro_config(
    work_minutes: u32,
    break_minutes: u32,
) -> Result<serde_json::Value, String> {
    if work_minutes == 0 || break_minutes == 0 || work_minutes > 180 || break_minutes > 120 {
        return Err("时长需在 1-180 / 1-120 分钟内".into());
    }
    let _ = POMODORO.set(aurora_core::l3_domain::today_view::PomodoroTimer::new(
        work_minutes,
        break_minutes,
    ));
    Ok(pomo_state_json())
}

/// DK-22 DoD：WebView 后台化回归补偿——按挂钟锚差值一次性补秒。
#[tauri::command]
async fn cmd_pomodoro_sync() -> Result<serde_json::Value, String> {
    pomodoro().sync_wall_clock();
    Ok(pomo_state_json())
}

/// DK-25 周回顾：全周口径——`week_start`（YYYY-MM-DD，须为周一；缺省=本周一）
/// 起算 [周一 00:00, 下周一 00:00) 闭区间内完成任务集，聚合预计 vs 实际偏差率。
#[tauri::command]
async fn cmd_weekly_review(week_start: Option<String>) -> Result<serde_json::Value, String> {
    use chrono::{Datelike, Duration, NaiveDate, Weekday};
    let core = get_core()?;
    let proj = task_proj(&core)?;

    // 解析周一锚点（缺省 = 本周周一；非周一输入回退到所在周的周一）
    let (monday, label) = match week_start.as_deref() {
        Some(s) => {
            let d = NaiveDate::parse_from_str(s, "%Y-%m-%d")
                .map_err(|e| format!("week_start 需为 YYYY-MM-DD: {e}"))?;
            let back = d.weekday().num_days_from_monday() as i64;
            (d - Duration::days(back), s.to_string())
        }
        None => {
            let today = chrono::Utc::now().date_naive();
            let back = today.weekday().num_days_from_monday() as i64;
            (today - Duration::days(back), String::new())
        }
    };
    let from_ms = monday
        .and_hms_opt(0, 0, 0)
        .ok_or("monday time")?
        .and_utc()
        .timestamp_millis();
    let to_ms = (monday + Duration::days(7))
        .and_hms_opt(0, 0, 0)
        .ok_or("next monday time")?
        .and_utc()
        .timestamp_millis()
        - 1; // 周日 23:59:59.999 含（闭区间）
    let next_monday = (monday + Duration::days(7)).format("%Y-%m-%d").to_string();
    let range_label = if label.is_empty() {
        format!("{} ~ {}", monday, next_monday)
    } else {
        format!("{label} ~ {next_monday}")
    };

    let ids = proj.completed_task_ids_between(from_ms, to_ms);
    let s = proj.weekly_summary(&ids);
    Ok(serde_json::json!({
        "range_label": range_label,
        "completed_count": ids.len(),
        "total_estimate_minutes": s.total_estimate_minutes,
        "total_actual_minutes": s.total_actual_minutes,
        "deviation_rate": s.deviation_rate,
        "per_task": s.per_task,
    }))
}

/// 反向链接（移动端 get_backlinks 同源 — 双链投影 incoming + 标题解析）。
#[tauri::command]
async fn cmd_get_backlinks(note_id: String) -> Result<Vec<serde_json::Value>, String> {
    let core = get_core()?;
    let kv = core.kv_store.clone();
    let sources = core.bidi_link_incoming(&note_id);
    let mut out = Vec::with_capacity(sources.len());
    for src in sources {
        // 标题解析: 与移动端同语义 — 读不到正文时回退显示 ID
        let title = kv
            .get(&format!("note:{}", src))
            .await
            .ok()
            .flatten()
            .and_then(|bytes| {
                serde_json::from_slice::<serde_json::Value>(&bytes)
                    .ok()
                    .and_then(|v| v.get("title").and_then(|t| t.as_str().map(String::from)))
            })
            .unwrap_or_else(|| src.clone());
        out.push(serde_json::json!({
            "source_note_id": src,
            "source_title": title,
        }));
    }
    Ok(out)
}

/// FSRS 到期复习卡（移动端 due_review_cards 同源 — 复习队列 due/R 阈值）。
#[tauri::command]
async fn cmd_due_review_cards() -> Result<Vec<serde_json::Value>, String> {
    let core = get_core()?;
    let now = chrono::Utc::now();
    let items = core.review_queue.due_items(now);
    let scheduler = aurora_core::l3_domain::fsrs::FsrsScheduler::new();
    Ok(items
        .into_iter()
        .map(|it| {
            let r = scheduler.retrievability(&it.state, now);
            serde_json::json!({
                "card_id": it.card_id,
                "note_id": it.note_id,
                "due_at": it.due.to_rfc3339(),
                "retrievability": r,
                "reps": it.state.reps,
                "lapses": it.state.lapses,
            })
        })
        .collect())
}

/// FSRS 评分复习（移动端 review_card 同源 — 1 Again / 2 Hard / 3 Good / 4 Easy）。
#[tauri::command]
async fn cmd_review_card(card_id: String, rating: i64) -> Result<String, String> {
    let core = get_core()?;
    let rating = match rating {
        1 => aurora_core::l3_domain::fsrs::Rating::Again,
        2 => aurora_core::l3_domain::fsrs::Rating::Hard,
        3 => aurora_core::l3_domain::fsrs::Rating::Good,
        4 => aurora_core::l3_domain::fsrs::Rating::Easy,
        _ => return Err("rating must be 1-4".into()),
    };
    let out = core
        .review_queue
        .review_card(&card_id, rating)
        .ok_or_else(|| format!("card not found: {}", card_id))?;
    Ok(out.due.to_rfc3339())
}

/// 删除笔记（含搜索索引）。
#[tauri::command]
async fn cmd_delete_note(note_id: String) -> Result<(), String> {
    let core = get_core()?;
    let vault = get_vault()?;
    let blocks = blocks_store();
    // V26 I2/DK-01W: 走 WritePath（元数据+快照同删 + NoteDeleted 事件）
    // V26 DK-01W: owned ctx（'static boxed 闭包，避免 async 自借用）
    let crypto = core.crypto.clone();
    let vault_seal = vault.clone();
    let seal = move |b: &[u8]| {
        vault_seal
            .encrypt(crypto.as_ref(), b)
            .map_err(|e| aurora_core::Error::Internal(e.to_string()))
    };
    let crypto2 = core.crypto.clone();
    let vault_unseal = vault.clone();
    let unseal = move |b: &[u8]| {
        vault_unseal
            .decrypt(crypto2.as_ref(), b)
            .map_err(|e| aurora_core::Error::Internal(e.to_string()))
    };
    let ctx = aurora_core::write_path::WriteContext {
        core: core.clone(),
        blocks,
        seal: Some(aurora_core::write_path::SealPair {
            seal: Box::new(seal),
            unseal: Box::new(unseal),
        }),
        content_cipher: Some(content_cipher_pair()?),
        attachments: ATTACH_STATE
            .lock()
            .expect("ATTACH_STATE mutex poisoned")
            .clone(), // DK-09
    };
    aurora_core::write_path::delete_note(&ctx, &note_id)
        .await
        .map_err(|e| e.to_string())?;
    core.catch_up_projections()
        .await
        .map_err(|e| e.to_string())?;
    info!(note_id = %note_id, "note deleted via desktop WritePath");
    Ok(())
}

/// 列出回收站（DK-02 S1）。
#[tauri::command]
async fn cmd_list_trashed() -> Result<Vec<serde_json::Value>, String> {
    let core = get_core()?;
    let items = aurora_core::write_path::list_trashed(&core)
        .await
        .map_err(|e| e.to_string())?;
    Ok(items
        .into_iter()
        .map(|t| {
            serde_json::json!({
                "note_id": t.note_id,
                "deleted_at_ms": t.deleted_at_ms,
                "title": t.title,
            })
        })
        .collect())
}

/// 创建文件夹（DK-02 S2）。
#[tauri::command]
async fn cmd_create_folder(
    parent_id: Option<String>,
    title: String,
) -> Result<serde_json::Value, String> {
    let core = get_core()?;
    let vault = get_vault()?;
    let blocks = blocks_store();
    let crypto = core.crypto.clone();
    let vault_seal = vault.clone();
    let seal = move |b: &[u8]| {
        vault_seal
            .encrypt(crypto.as_ref(), b)
            .map_err(|e| aurora_core::Error::Internal(e.to_string()))
    };
    let crypto2 = core.crypto.clone();
    let vault_unseal = vault.clone();
    let unseal = move |b: &[u8]| {
        vault_unseal
            .decrypt(crypto2.as_ref(), b)
            .map_err(|e| aurora_core::Error::Internal(e.to_string()))
    };
    let ctx = aurora_core::write_path::WriteContext {
        core: core.clone(),
        blocks,
        seal: Some(aurora_core::write_path::SealPair {
            seal: Box::new(seal),
            unseal: Box::new(unseal),
        }),
        content_cipher: Some(content_cipher_pair()?),
        attachments: ATTACH_STATE
            .lock()
            .expect("ATTACH_STATE mutex poisoned")
            .clone(),
    };
    let rc = aurora_core::write_path::create_folder(&ctx, parent_id.as_deref(), &title)
        .await
        .map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "note_id": rc.aggregate_id,
        "seq": rc.seq,
    }))
}

/// 移动节点（DK-02 S2——环检测 fail-closed）。
#[tauri::command]
async fn cmd_move_note(
    note_id: String,
    new_parent_id: Option<String>,
    sort_order: i64,
) -> Result<(), String> {
    let core = get_core()?;
    let vault = get_vault()?;
    let blocks = blocks_store();
    let crypto = core.crypto.clone();
    let vault_seal = vault.clone();
    let seal = move |b: &[u8]| {
        vault_seal
            .encrypt(crypto.as_ref(), b)
            .map_err(|e| aurora_core::Error::Internal(e.to_string()))
    };
    let crypto2 = core.crypto.clone();
    let vault_unseal = vault.clone();
    let unseal = move |b: &[u8]| {
        vault_unseal
            .decrypt(crypto2.as_ref(), b)
            .map_err(|e| aurora_core::Error::Internal(e.to_string()))
    };
    let ctx = aurora_core::write_path::WriteContext {
        core: core.clone(),
        blocks,
        seal: Some(aurora_core::write_path::SealPair {
            seal: Box::new(seal),
            unseal: Box::new(unseal),
        }),
        content_cipher: None,
        attachments: ATTACH_STATE
            .lock()
            .expect("ATTACH_STATE mutex poisoned")
            .clone(),
    };
    aurora_core::write_path::move_node(&ctx, &note_id, new_parent_id.as_deref(), sort_order)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// 重命名文件夹（DK-02 S2）。
#[tauri::command]
async fn cmd_rename_folder(folder_id: String, new_title: String) -> Result<(), String> {
    let core = get_core()?;
    let vault = get_vault()?;
    let blocks = blocks_store();
    let crypto = core.crypto.clone();
    let vault_seal = vault.clone();
    let seal = move |b: &[u8]| {
        vault_seal
            .encrypt(crypto.as_ref(), b)
            .map_err(|e| aurora_core::Error::Internal(e.to_string()))
    };
    let crypto2 = core.crypto.clone();
    let vault_unseal = vault.clone();
    let unseal = move |b: &[u8]| {
        vault_unseal
            .decrypt(crypto2.as_ref(), b)
            .map_err(|e| aurora_core::Error::Internal(e.to_string()))
    };
    let ctx = aurora_core::write_path::WriteContext {
        core: core.clone(),
        blocks,
        seal: Some(aurora_core::write_path::SealPair {
            seal: Box::new(seal),
            unseal: Box::new(unseal),
        }),
        content_cipher: None,
        attachments: ATTACH_STATE
            .lock()
            .expect("ATTACH_STATE mutex poisoned")
            .clone(),
    };
    aurora_core::write_path::rename_folder(&ctx, &folder_id, &new_title)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// 目录树扁平列表（DK-02 S2——UI 端组装嵌套）。
#[tauri::command]
async fn cmd_list_tree() -> Result<Vec<serde_json::Value>, String> {
    let core = get_core()?;
    let vault = get_vault()?;
    let crypto = core.crypto.clone();
    let vault_unseal = vault.clone();
    let unseal = move |b: &[u8]| {
        vault_unseal
            .decrypt(crypto.as_ref(), b)
            .map_err(|e| aurora_core::Error::Internal(e.to_string()))
    };
    let seal_pair = aurora_core::write_path::SealPair {
        seal: Box::new(|b: &[u8]| Ok(b.to_vec())),
        unseal: Box::new(unseal),
    };
    let nodes = aurora_core::write_path::list_tree(&core, Some(&seal_pair))
        .await
        .map_err(|e| e.to_string())?;
    Ok(nodes
        .into_iter()
        .map(|n| {
            serde_json::json!({
                "note_id": n.note_id,
                "kind": format!("{:?}", n.kind),
                "title": n.title,
                "parent_id": n.parent_id,
                "sort_order": n.sort_order,
            })
        })
        .collect())
}

/// 从回收站恢复笔记（DK-02 S1 — NoteCreated 重放驱动投影/索引重建）。
#[tauri::command]
async fn cmd_restore_note(note_id: String) -> Result<(), String> {
    let core = get_core()?;
    let vault = get_vault()?;
    let blocks = blocks_store();
    let crypto = core.crypto.clone();
    let vault_seal = vault.clone();
    let seal = move |b: &[u8]| {
        vault_seal
            .encrypt(crypto.as_ref(), b)
            .map_err(|e| aurora_core::Error::Internal(e.to_string()))
    };
    let crypto2 = core.crypto.clone();
    let vault_unseal = vault.clone();
    let unseal = move |b: &[u8]| {
        vault_unseal
            .decrypt(crypto2.as_ref(), b)
            .map_err(|e| aurora_core::Error::Internal(e.to_string()))
    };
    let ctx = aurora_core::write_path::WriteContext {
        core: core.clone(),
        blocks,
        seal: Some(aurora_core::write_path::SealPair {
            seal: Box::new(seal),
            unseal: Box::new(unseal),
        }),
        content_cipher: None, // 与 cmd_delete_note 同口径（TODO(DK-07)）
        attachments: ATTACH_STATE
            .lock()
            .expect("ATTACH_STATE mutex poisoned")
            .clone(),
    };
    aurora_core::write_path::restore_note(&ctx, &note_id)
        .await
        .map_err(|e| e.to_string())?;
    core.catch_up_projections()
        .await
        .map_err(|e| e.to_string())?;
    info!(note_id = %note_id, "note restored via desktop command");
    Ok(())
}

/// 彻底删除回收站笔记（DK-02 S1 — 物理删 + 附件级联）。
#[tauri::command]
async fn cmd_purge_note(note_id: String) -> Result<(), String> {
    let core = get_core()?;
    let vault = get_vault()?;
    let blocks = blocks_store();
    let crypto = core.crypto.clone();
    let vault_seal = vault.clone();
    let seal = move |b: &[u8]| {
        vault_seal
            .encrypt(crypto.as_ref(), b)
            .map_err(|e| aurora_core::Error::Internal(e.to_string()))
    };
    let crypto2 = core.crypto.clone();
    let vault_unseal = vault.clone();
    let unseal = move |b: &[u8]| {
        vault_unseal
            .decrypt(crypto2.as_ref(), b)
            .map_err(|e| aurora_core::Error::Internal(e.to_string()))
    };
    let ctx = aurora_core::write_path::WriteContext {
        core: core.clone(),
        blocks,
        seal: Some(aurora_core::write_path::SealPair {
            seal: Box::new(seal),
            unseal: Box::new(unseal),
        }),
        content_cipher: None,
        attachments: ATTACH_STATE
            .lock()
            .expect("ATTACH_STATE mutex poisoned")
            .clone(),
    };
    aurora_core::write_path::purge_note(&ctx, &note_id)
        .await
        .map_err(|e| e.to_string())?;
    // DK-03 S3b：purge 联动——向量记录物理清（残留会命中已删笔记）
    if let Ok(vecs) = get_vectors() {
        if let Err(e) = vecs.remove_note(&note_id).await {
            warn!(note_id = %note_id, error = %e, "vector remove_note failed (purge)");
        }
    }
    core.catch_up_projections()
        .await
        .map_err(|e| e.to_string())?;
    info!(note_id = %note_id, "note purged via desktop command");
    Ok(())
}

/// DK-17 遗留补全：清理超期回收站（days 天前的标记全 purge——TrashView「清理过期」按钮）。
#[tauri::command]
async fn cmd_purge_expired_trash(days: i64) -> Result<Vec<String>, String> {
    let core = get_core()?;
    let vault = get_vault()?;
    let blocks = blocks_store();
    let crypto = core.crypto.clone();
    let vault_seal = vault.clone();
    let seal = move |b: &[u8]| {
        vault_seal
            .encrypt(crypto.as_ref(), b)
            .map_err(|e| aurora_core::Error::Internal(e.to_string()))
    };
    let crypto2 = core.crypto.clone();
    let vault_unseal = vault.clone();
    let unseal = move |b: &[u8]| {
        vault_unseal
            .decrypt(crypto2.as_ref(), b)
            .map_err(|e| aurora_core::Error::Internal(e.to_string()))
    };
    let ctx = aurora_core::write_path::WriteContext {
        core: core.clone(),
        blocks,
        seal: Some(aurora_core::write_path::SealPair {
            seal: Box::new(seal),
            unseal: Box::new(unseal),
        }),
        content_cipher: None,
        attachments: ATTACH_STATE
            .lock()
            .expect("ATTACH_STATE mutex poisoned")
            .clone(),
    };
    let purged = aurora_core::write_path::purge_expired(&ctx, days)
        .await
        .map_err(|e| e.to_string())?;
    // DK-03 S3b：purge 联动——向量记录物理清（trash 面过滤随物理删失效，须清）
    if let Ok(vecs) = get_vectors() {
        for id in &purged {
            if let Err(e) = vecs.remove_note(id).await {
                warn!(note_id = %id, error = %e, "vector remove_note failed (purge)");
            }
        }
    }
    core.catch_up_projections()
        .await
        .map_err(|e| e.to_string())?;
    info!(
        days,
        count = purged.len(),
        "trash purge_expired via desktop command"
    );
    Ok(purged)
}

/// 打开当日每日笔记（DK-19——启动/访问当日视图调用；Auto 自动创建，Manual 仅定位）。
#[tauri::command]
async fn cmd_daily_note_open(date: String) -> Result<serde_json::Value, String> {
    let core = get_core()?;
    let vault = get_vault()?;
    let blocks = blocks_store();
    let crypto = core.crypto.clone();
    let vault_seal = vault.clone();
    let seal = move |b: &[u8]| {
        vault_seal
            .encrypt(crypto.as_ref(), b)
            .map_err(|e| aurora_core::Error::Internal(e.to_string()))
    };
    let crypto2 = core.crypto.clone();
    let vault_unseal = vault.clone();
    let unseal = move |b: &[u8]| {
        vault_unseal
            .decrypt(crypto2.as_ref(), b)
            .map_err(|e| aurora_core::Error::Internal(e.to_string()))
    };
    let ctx = aurora_core::write_path::WriteContext {
        core: core.clone(),
        blocks,
        seal: Some(aurora_core::write_path::SealPair {
            seal: Box::new(seal),
            unseal: Box::new(unseal),
        }),
        content_cipher: None,
        attachments: ATTACH_STATE
            .lock()
            .expect("ATTACH_STATE mutex poisoned")
            .clone(),
    };
    match aurora_core::write_path::ensure_daily_note(&ctx, &date).await {
        Ok(Some(r)) => Ok(serde_json::json!({ "note_id": r.note_id, "created": r.created })),
        Ok(None) => Ok(serde_json::json!({ "note_id": serde_json::Value::Null, "created": false })),
        Err(e) => Err(e.to_string()),
    }
}

/// 设置每日笔记模式（DK-19：auto/manual/off）。
#[tauri::command]
async fn cmd_daily_note_set_mode(mode: String) -> Result<(), String> {
    let core = get_core()?;
    let m = match mode.as_str() {
        "auto" => aurora_core::write_path::DailyNoteMode::Auto,
        "manual" => aurora_core::write_path::DailyNoteMode::Manual,
        "off" => aurora_core::write_path::DailyNoteMode::Off,
        _ => return Err(format!("非法模式: '{mode}'（需 auto/manual/off）")),
    };
    aurora_core::write_path::set_daily_mode(&core, m)
        .await
        .map_err(|e| e.to_string())
}

/// 读取每日笔记模式（DK-19）。
#[tauri::command]
async fn cmd_daily_note_get_mode() -> Result<String, String> {
    let core = get_core()?;
    let m = aurora_core::write_path::get_daily_mode(&core)
        .await
        .map_err(|e| e.to_string())?;
    Ok(match m {
        aurora_core::write_path::DailyNoteMode::Auto => "auto",
        aurora_core::write_path::DailyNoteMode::Manual => "manual",
        aurora_core::write_path::DailyNoteMode::Off => "off",
    }
    .to_string())
}

/// 设置每日笔记模板（DK-19，占位符 {{date}}/{{weekday}}）。
#[tauri::command]
async fn cmd_daily_note_set_template(template: String) -> Result<(), String> {
    let core = get_core()?;
    aurora_core::write_path::set_daily_template(&core, &template)
        .await
        .map_err(|e| e.to_string())
}

/// 读取每日笔记模板（DK-19，未设置返回默认模板）。
#[tauri::command]
async fn cmd_daily_note_get_template() -> Result<String, String> {
    let core = get_core()?;
    aurora_core::write_path::get_daily_template(&core)
        .await
        .map_err(|e| e.to_string())
}

/// 创建智能文件夹（DK-02 S3）。
#[tauri::command]
async fn cmd_smartfolder_create(
    parent_id: Option<String>,
    title: String,
    title_contains: Option<String>,
) -> Result<serde_json::Value, String> {
    let core = get_core()?;
    let vault = get_vault()?;
    let blocks = blocks_store();
    let crypto = core.crypto.clone();
    let vault_seal = vault.clone();
    let seal = move |b: &[u8]| {
        vault_seal
            .encrypt(crypto.as_ref(), b)
            .map_err(|e| aurora_core::Error::Internal(e.to_string()))
    };
    let crypto2 = core.crypto.clone();
    let vault_unseal = vault.clone();
    let unseal = move |b: &[u8]| {
        vault_unseal
            .decrypt(crypto2.as_ref(), b)
            .map_err(|e| aurora_core::Error::Internal(e.to_string()))
    };
    let ctx = aurora_core::write_path::WriteContext {
        core: core.clone(),
        blocks,
        seal: Some(aurora_core::write_path::SealPair {
            seal: Box::new(seal),
            unseal: Box::new(unseal),
        }),
        content_cipher: None,
        attachments: ATTACH_STATE
            .lock()
            .expect("ATTACH_STATE mutex poisoned")
            .clone(),
    };
    let rc = aurora_core::write_path::create_smartfolder(
        &ctx,
        parent_id.as_deref(),
        &title,
        aurora_core::write_path::FilterRule { title_contains },
    )
    .await
    .map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "note_id": rc.aggregate_id, "seq": rc.seq }))
}

/// 更新智能文件夹规则（DK-02 S3）。
#[tauri::command]
async fn cmd_smartfolder_set_rule(
    folder_id: String,
    title_contains: Option<String>,
) -> Result<(), String> {
    let core = get_core()?;
    let vault = get_vault()?;
    let blocks = blocks_store();
    let crypto = core.crypto.clone();
    let vault_seal = vault.clone();
    let seal = move |b: &[u8]| {
        vault_seal
            .encrypt(crypto.as_ref(), b)
            .map_err(|e| aurora_core::Error::Internal(e.to_string()))
    };
    let crypto2 = core.crypto.clone();
    let vault_unseal = vault.clone();
    let unseal = move |b: &[u8]| {
        vault_unseal
            .decrypt(crypto2.as_ref(), b)
            .map_err(|e| aurora_core::Error::Internal(e.to_string()))
    };
    let ctx = aurora_core::write_path::WriteContext {
        core: core.clone(),
        blocks,
        seal: Some(aurora_core::write_path::SealPair {
            seal: Box::new(seal),
            unseal: Box::new(unseal),
        }),
        content_cipher: None,
        attachments: ATTACH_STATE
            .lock()
            .expect("ATTACH_STATE mutex poisoned")
            .clone(),
    };
    aurora_core::write_path::update_rule(
        &ctx,
        &folder_id,
        aurora_core::write_path::FilterRule { title_contains },
    )
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// 读取智能文件夹规则（DK-02 S3 UI 面——Alpha 跨领地补丁：仅 tauri 读端薄层，
/// 复用 unwrap_note_bytes 既有解密模式，write_path/core 零触碰——领地声明见回执）。
#[tauri::command]
async fn cmd_smartfolder_get_rule(folder_id: String) -> Result<serde_json::Value, String> {
    let core = get_core()?;
    let vault = get_vault()?;
    let data = core
        .kv_store
        .get(&format!("note:{folder_id}"))
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("smartfolder not found: {folder_id}"))?;
    let json = unwrap_note_bytes(&core, &vault, &data)?;
    let rule = json
        .get("rule")
        .cloned()
        .unwrap_or(serde_json::json!({ "title_contains": null }));
    Ok(serde_json::json!({
        "title_contains": rule.get("title_contains").and_then(|v| v.as_str()),
    }))
}

/// 求值智能文件夹（DK-02 S3——动态视图实时列表）。
#[tauri::command]
async fn cmd_smartfolder_list_items(folder_id: String) -> Result<Vec<serde_json::Value>, String> {
    let core = get_core()?;
    let vault = get_vault()?;
    let crypto = core.crypto.clone();
    let vault_unseal = vault.clone();
    let unseal = move |b: &[u8]| {
        vault_unseal
            .decrypt(crypto.as_ref(), b)
            .map_err(|e| aurora_core::Error::Internal(e.to_string()))
    };
    let seal_pair = aurora_core::write_path::SealPair {
        seal: Box::new(|b: &[u8]| Ok(b.to_vec())),
        unseal: Box::new(unseal),
    };
    let items =
        aurora_core::write_path::evaluate_smart_folder_for(&core, Some(&seal_pair), &folder_id)
            .await
            .map_err(|e| e.to_string())?;
    Ok(items
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "note_id": r.id,
                "title": r.title,
                "updated_at": r.updated_at,
            })
        })
        .collect())
}

/// 搜索笔记（Tantivy 全文检索；mode=hybrid 走 BM25+KNN→RRF 混合路）。
#[tauri::command]
async fn cmd_search_notes(
    query: String,
    mode: Option<String>,
) -> Result<Vec<serde_json::Value>, String> {
    use aurora_core::traits::search_backend::SearchOptions;
    let core = get_core()?;
    info!(
        query,
        mode = mode.as_deref().unwrap_or("bm25"),
        "search notes via desktop command"
    );
    let opts = SearchOptions {
        limit: 20,
        ..SearchOptions::default()
    };
    let (hits, total) = if mode.as_deref() == Some("hybrid") {
        let hybrid = get_hybrid()?;
        let r = hybrid
            .search_hybrid(&query, &opts)
            .await
            .map_err(|e| e.to_string())?;
        let n = r.hits.len();
        (r.hits, n)
    } else {
        let r = core
            .search
            .search(&query, &opts)
            .await
            .map_err(|e| e.to_string())?;
        (r.hits, r.total)
    };
    let vault = get_vault()?;
    let mut results = Vec::with_capacity(hits.len());
    for hit in hits {
        // title 实时回填：BM25 命中自带；纯向量命中 title 留空——此处解密读
        // note 元数据补全（改标题不触发重嵌，title 不入向量记录——见 S3a 注释）。
        let title = if hit.title.is_empty() {
            core.kv_store
                .get(&format!("note:{}", hit.note_id))
                .await
                .ok()
                .flatten()
                .and_then(|bytes| unwrap_note_bytes(&core, &vault, &bytes).ok())
                .and_then(|v| v.get("title").and_then(|t| t.as_str().map(String::from)))
                .unwrap_or_default()
        } else {
            hit.title.clone()
        };
        results.push(serde_json::json!({
            "note_id": hit.note_id,
            "doc_id": hit.note_id, // 前端 CommandPalette 契约为 note_id——doc_id 为历史键，双写兼容
            "title": title,
            "score": hit.score,
            "snippet": hit.snippet,
            "source": format!("{:?}", hit.source), // Bm25 / Vector / Both（徽章数据面）
        }));
    }
    let _ = total; // RRF 截断后 total 无分页语义（hybrid）；BM25 分页留后续卡
    Ok(results)
}

/// 获取应用状态摘要（健康检查）。
#[tauri::command]
fn cmd_app_status() -> Result<serde_json::Value, String> {
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
