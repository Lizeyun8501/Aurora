//! Aurora Note Mobile FFI (UniFFI)
//!
//! 对应 V19 §28.1 Trait 签名的移动端适配，提供跨平台 FFI 入口：
//! - iOS/Android 通过 `UniffiAppCore` 访问核心功能
//! - 方法均为同步阻塞式（符合 V19 保留同步签名的决策）

use std::sync::{Arc, Mutex};

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

/// 移动端应用核心 FFI 包装。
///
/// 内部持有 `aurora_core::AppCore` 实例，通过 Mutex 保证线程安全。
/// 所有方法均为同步阻塞式，调用方（Swift/Kotlin）需在后台线程调用。
#[derive(uniffi::Object)]
pub struct UniffiAppCore {
    // 当前阶段使用简化占位实现；后续接入真实 AppCore。
    notes: Mutex<Vec<NoteSummary>>,
}

#[uniffi::export]
impl UniffiAppCore {
    /// 创建新的移动端应用核心实例。
    ///
    /// # Arguments
    /// * `data_dir` — 移动端沙盒数据目录路径。
    #[uniffi::constructor]
    pub fn new(data_dir: String) -> Result<Arc<Self>, MobileError> {
        // TODO: 后续接入真实 AppCoreBuilder + startup()
        // let core = aurora_core::AppCoreBuilder::new()
        //     .data_dir(data_dir)
        //     .build()
        //     .map_err(|e| MobileError::InitFailed { message: e.to_string() })?;
        let _ = data_dir;
        Ok(Arc::new(Self {
            notes: Mutex::new(Vec::new()),
        }))
    }

    /// 创建新笔记。
    pub fn create_note(self: Arc<Self>, title: String) -> Result<String, MobileError> {
        let id = uuid::Uuid::new_v4().to_string();
        let mut notes = self.notes.lock().unwrap();
        notes.push(NoteSummary {
            id: id.clone(),
            title,
            updated_at: chrono::Utc::now().to_rfc3339(),
        });
        Ok(id)
    }

    /// 列出所有笔记。
    pub fn list_notes(self: Arc<Self>) -> Vec<NoteSummary> {
        let notes = self.notes.lock().unwrap();
        notes.clone()
    }

    /// 搜索笔记（FTS 简化版本）。
    pub fn search_notes(self: Arc<Self>, query: String) -> Vec<SearchResult> {
        let notes = self.notes.lock().unwrap();
        notes
            .iter()
            .filter(|n| n.title.to_lowercase().contains(&query.to_lowercase()))
            .map(|n| SearchResult {
                note_id: n.id.clone(),
                title: n.title.clone(),
                snippet: n.title.clone(),
                score: 1.0,
            })
            .collect()
    }

    /// 删除笔记。
    pub fn delete_note(self: Arc<Self>, note_id: String) -> Result<(), MobileError> {
        let mut notes = self.notes.lock().unwrap();
        let pos = notes.iter().position(|n| n.id == note_id);
        match pos {
            Some(i) => {
                notes.remove(i);
                Ok(())
            }
            None => Err(MobileError::NotFound {
                resource: note_id,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_lifecycle() {
        let core = UniffiAppCore::new("/tmp/aurora-test".into()).unwrap();
        let id = core.create_note("Test Note".into()).unwrap();
        let notes = core.list_notes();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].id, id);

        let results = core.search_notes("test".into());
        assert_eq!(results.len(), 1);

        core.delete_note(id).unwrap();
        assert!(core.list_notes().is_empty());
    }
}
