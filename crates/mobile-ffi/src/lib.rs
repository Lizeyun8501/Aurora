//! Aurora Note Mobile FFI (UniFFI)
//!
//! 对应 V19 §28.1 Trait 签名的移动端适配，提供跨平台 FFI 入口：
//! - iOS/Android 通过 `UniffiAppCore` 访问核心功能
//! - 方法均为同步阻塞式（符合 V19 保留同步签名的决策），内部持有独立
//!   tokio runtime 驱动异步核心调用；调用方（Swift/Kotlin）需在后台线程调用
//! - 启动装配复用 [`aurora_bootstrap`]（迁移 + DEK 保险库 + AppCore DI 注入 +
//!   startup），与桌面端保持一致
//! - 笔记内容与桌面端一致：明文 JSON → AES-256-GCM 加密 → KVStore 落库，
//!   同时同步建立本地全文检索索引（明文索引仅存本机）

use std::path::Path;
use std::sync::Arc;

use aurora_core::app_core::AppCore;
use aurora_core::traits::kv_store::KVStore;
use aurora_core::traits::search_backend::SearchBackend;
use aurora_security::LocalDekVault;
use tracing::warn;

uniffi::setup_scaffolding!();

/// 移动端错误类型。
#[derive(uniffi::Error, Debug, thiserror::Error)]
pub enum MobileError {
    #[error("core init failed: {message}")]
    InitFailed { message: String },
    #[error("operation failed: {message}")]
    OperationFailed { message: String },
    #[error("not found: {resource}")]
    NotFound { resource: String },
}

/// FFI 友好的笔记摘要（不含大字段）。
#[derive(uniffi::Record, Debug, Clone)]
pub struct NoteSummary {
    pub id: String,
    pub title: String,
    pub updated_at: String,
}

/// FFI 友好的搜索结果。
#[derive(uniffi::Record, Debug, Clone)]
pub struct SearchResult {
    pub note_id: String,
    pub title: String,
    pub snippet: String,
    pub score: f64,
}

/// KVStore 中笔记键的前缀。
const NOTE_KEY_PREFIX: &str = "note:";
/// 默认工作区 ID（单工作区模式；多工作区接入后改为按用户选择注入）。
const DEFAULT_WORKSPACE_ID: &str = "default";

/// 移动端应用核心 FFI 包装。
///
/// 内部持有 `aurora_core::AppCore` 实例与本地 DEK 保险库（E2EE），
/// 通过独立 tokio runtime 以同步阻塞方式驱动异步核心调用。
pub struct UniffiAppCore {
    core: Arc<AppCore>,
    vault: Arc<LocalDekVault>,
    /// 同步 FFI 方法内部用于 block_on 异步核心调用的运行时。
    runtime: tokio::runtime::Runtime,
}

#[uniffi::export]
impl UniffiAppCore {
    /// 创建新的移动端应用核心实例。
    ///
    /// # Arguments
    /// * `data_dir` — 移动端沙盒数据目录路径。
    #[uniffi::constructor]
    pub fn new(data_dir: String) -> Result<Arc<Self>, MobileError> {
        let app = aurora_bootstrap::bootstrap(Path::new(&data_dir)).map_err(|e| {
            MobileError::InitFailed {
                message: e.to_string(),
            }
        })?;
        let runtime = tokio::runtime::Runtime::new().map_err(|e| MobileError::InitFailed {
            message: format!("tokio runtime init failed: {}", e),
        })?;
        Ok(Arc::new(Self {
            core: app.core,
            vault: app.vault,
            runtime,
        }))
    }

    /// 创建新笔记（加密存储 + 建立搜索索引）。
    pub fn create_note(self: Arc<Self>, title: String) -> Result<String, MobileError> {
        let id = uuid::Uuid::new_v4().to_string();
        let note = serde_json::json!({
            "id": id,
            "title": title,
            "content": "",
            "content_type": "markdown",
            "created_at": chrono::Utc::now().to_rfc3339(),
        });
        let sealed = self.seal_note(&note)?;
        let key = format!("{}{}", NOTE_KEY_PREFIX, id);
        self.runtime
            .block_on(async { self.core.kv_store.set(&key, &sealed).await })
            .map_err(op_err)?;
        self.runtime
            .block_on(async {
                self.core
                    .search
                    .index_note(&id, "", &self.note_metadata(&note))
                    .await
            })
            .map_err(op_err)?;
        Ok(id)
    }

    /// 列出所有笔记（从加密存储解密后返回摘要）。
    pub fn list_notes(self: Arc<Self>) -> Vec<NoteSummary> {
        let items = match self
            .runtime
            .block_on(async { self.core.kv_store.scan_prefix(NOTE_KEY_PREFIX).await })
        {
            Ok(v) => v,
            Err(e) => {
                warn!(error = %e, "list_notes scan failed");
                return Vec::new();
            }
        };
        items
            .into_iter()
            .filter_map(|(key, data)| {
                let id = key
                    .strip_prefix(NOTE_KEY_PREFIX)
                    .unwrap_or(key.as_str())
                    .to_string();
                match self.unwrap_note(&data) {
                    Ok(note) => Some(NoteSummary {
                        id,
                        title: note
                            .get("title")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        updated_at: note
                            .get("updated_at")
                            .or_else(|| note.get("created_at"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                    }),
                    Err(e) => {
                        warn!(note_id = %id, error = %e, "skipping undecryptable note");
                        None
                    }
                }
            })
            .collect()
    }

    /// 搜索笔记（Tantivy 全文检索）。
    pub fn search_notes(self: Arc<Self>, query: String) -> Vec<SearchResult> {
        let opts = aurora_core::traits::search_backend::SearchOptions::default();
        match self
            .runtime
            .block_on(async { self.core.search.search(&query, &opts).await })
        {
            Ok(result) => result
                .hits
                .into_iter()
                .map(|h| SearchResult {
                    note_id: h.note_id,
                    title: h.title,
                    snippet: h.snippet,
                    score: h.score as f64,
                })
                .collect(),
            Err(e) => {
                warn!(error = %e, "search_notes failed");
                Vec::new()
            }
        }
    }

    /// 删除笔记（含搜索索引；幂等）。
    pub fn delete_note(self: Arc<Self>, note_id: String) -> Result<(), MobileError> {
        let key = format!("{}{}", NOTE_KEY_PREFIX, note_id);
        self.runtime
            .block_on(async { self.core.kv_store.delete(&key).await })
            .map_err(op_err)?;
        self.runtime
            .block_on(async { self.core.search.remove_index(&note_id).await })
            .map_err(op_err)?;
        Ok(())
    }
}

// ── 内部辅助（E2EE + 索引元数据） ──────────────────────

impl UniffiAppCore {
    /// 加密笔记明文为落库字节。
    fn seal_note(&self, note: &serde_json::Value) -> Result<Vec<u8>, MobileError> {
        let payload = serde_json::to_vec(note).map_err(op_err)?;
        self.vault
            .encrypt(self.core.crypto.as_ref(), &payload)
            .map_err(op_err)
    }

    /// 解密笔记字节：优先按密文解密；解密失败时兼容升级前的明文 JSON
    /// （旧版本无加密落库），保证存量数据不丢失。
    fn unwrap_note(&self, data: &[u8]) -> Result<serde_json::Value, MobileError> {
        match self.vault.decrypt(self.core.crypto.as_ref(), data) {
            Ok(plaintext) => serde_json::from_slice(&plaintext).map_err(op_err),
            Err(e) => match serde_json::from_slice::<serde_json::Value>(data) {
                Ok(v) => {
                    warn!("note stored in legacy plaintext format; consider re-saving");
                    Ok(v)
                }
                Err(_) => Err(MobileError::OperationFailed {
                    message: format!("note decrypt failed: {}", e),
                }),
            },
        }
    }

    /// 构造搜索索引元数据（明文索引，V19 本地检索设计）。
    fn note_metadata(
        &self,
        note: &serde_json::Value,
    ) -> aurora_core::traits::search_backend::NoteMetadata {
        aurora_core::traits::search_backend::NoteMetadata {
            title: note
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            tags: vec![],
            workspace_id: DEFAULT_WORKSPACE_ID.to_string(),
            updated_at: Some(chrono::Utc::now()),
        }
    }
}

fn op_err(e: impl std::fmt::Display) -> MobileError {
    MobileError::OperationFailed {
        message: e.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_lifecycle() {
        let dir = tempfile::tempdir().unwrap();
        let core = UniffiAppCore::new(dir.path().to_string_lossy().into_owned()).unwrap();
        let id = core.create_note("Test Note".into()).unwrap();
        let notes = core.list_notes();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].id, id);
        assert_eq!(notes[0].title, "Test Note");

        let results = core.search_notes("test".into());
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].note_id, id);

        core.delete_note(id).unwrap();
        assert!(core.list_notes().is_empty());
    }
}
