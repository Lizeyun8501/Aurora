//! Aurora Note Mobile FFI (UniFFI + JNI)
//!
//! 对应 V19 §28.1 Trait 签名的移动端适配，提供跨平台 FFI 入口：
//! - iOS: 通过 UniFFI Record/Object 注解
//! - Android: 通过 JNI C ABI 桥接（Java_com_aurora_note_UniffiAppCore_*）
//!
//! 方法均为同步阻塞式（符合 V19 保留同步签名的决策）。
//! async Trait 方法（KVStore / SearchBackend）通过内部 tokio runtime 驱动。

use std::sync::{Arc, Mutex};
use std::path::PathBuf;

uniffi::setup_scaffolding!();

// ===========================================================================
// 类型定义（UniFFI 兼容）
// ===========================================================================

#[derive(uniffi::Error, Debug, thiserror::Error)]
pub enum MobileError {
    #[error("core init failed: {message}")]
    InitFailed { message: String },
    #[error("operation failed: {message}")]
    OperationFailed { message: String },
    #[error("not found: {resource}")]
    NotFound { resource: String },
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct NoteSummary {
    pub id: String,
    pub title: String,
    pub updated_at: String,
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct SearchResult {
    pub note_id: String,
    pub title: String,
    pub snippet: String,
    pub score: f64,
}

// ===========================================================================
// 内部数据模型（JSON 序列化用于 KVStore 持久化）
// ===========================================================================

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct NoteRecord {
    id: String,
    title: String,
    content: String,
    created_at: String,
    updated_at: String,
}

impl NoteRecord {
    fn new(title: String) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            title,
            content: String::new(),
            created_at: now.clone(),
            updated_at: now,
        }
    }

    fn to_summary(&self) -> NoteSummary {
        NoteSummary {
            id: self.id.clone(),
            title: self.title.clone(),
            updated_at: self.updated_at.clone(),
        }
    }
}

// ===========================================================================
// UniffiAppCore — 真实实现（接入 bootstrap + KVStore + SearchBackend）
// ===========================================================================

#[derive(uniffi::Object)]
pub struct UniffiAppCore {
    core: Option<Arc<aurora_core::app_core::AppCore>>,
    runtime: tokio::runtime::Runtime,
    data_dir: PathBuf,
    fallback_notes: Mutex<Vec<NoteRecord>>,
    is_fallback: bool,
}

impl UniffiAppCore {
    pub fn new(data_dir: String) -> Result<Arc<Self>, MobileError> {
        let data_dir = PathBuf::from(&data_dir);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| MobileError::InitFailed {
                message: format!("tokio runtime: {e}"),
            })?;

        // 尝试完整 bootstrap 装配
        match aurora_bootstrap::bootstrap(&data_dir) {
            Ok(booted) => {
                tracing::info!("bootstrap success — full mode");
                Ok(Arc::new(Self {
                    core: Some(booted.core),
                    runtime,
                    data_dir,
                    fallback_notes: Mutex::new(Vec::new()),
                    is_fallback: false,
                }))
            }
            Err(e) => {
                tracing::warn!("bootstrap failed, falling back to in-memory: {e}");
                Ok(Arc::new(Self {
                    core: None,
                    runtime,
                    data_dir,
                    fallback_notes: Mutex::new(Vec::new()),
                    is_fallback: true,
                }))
            }
        }
    }

    fn create_note_impl(self: &Arc<Self>, title: String) -> Result<String, MobileError> {
        let note = NoteRecord::new(title);

        if let Some(core) = &self.core {
            // 真实模式：KVStore 持久化 + SearchBackend 索引
            let kv = core.kv_store.clone();
            let key = format!("note:{}", note.id);
            let value = serde_json::to_vec(&note).map_err(|e| MobileError::OperationFailed {
                message: format!("serialize: {e}"),
            })?;

            self.runtime.block_on(async {
                kv.set(&key, &value).await
            }).map_err(|e| MobileError::OperationFailed {
                message: format!("kv set: {e}"),
            })?;

            // 索引笔记（忽略错误，搜索索引是可选的）
            let search = core.search.clone();
            let note_clone = note.clone();
            self.runtime.block_on(async {
                use aurora_core::traits::search_backend::NoteMetadata;
                let metadata = NoteMetadata {
                    title: note_clone.title.clone(),
                    tags: vec![],
                    workspace_id: String::new(),
                    updated_at: Some(chrono::DateTime::parse_from_rfc3339(&note_clone.updated_at)
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .unwrap_or_else(|_| chrono::Utc::now())),
                };
                search.index_note(&note_clone.id, &note_clone.content, &metadata).await
            }).ok();
        } else {
            // Fallback 模式：内存存储
            self.fallback_notes.lock().unwrap().push(note.clone());
        }

        Ok(note.id)
    }

    fn list_notes_impl(self: &Arc<Self>) -> Vec<NoteSummary> {
        if let Some(core) = &self.core {
            let kv = core.kv_store.clone();
            let entries = self.runtime.block_on(async {
                kv.scan_prefix("note:").await
            });

            match entries {
                Ok(pairs) => pairs
                    .iter()
                    .filter_map(|(_, bytes)| serde_json::from_slice::<NoteRecord>(bytes).ok())
                    .map(|n| n.to_summary())
                    .collect(),
                Err(_) => Vec::new(),
            }
        } else {
            self.fallback_notes.lock().unwrap()
                .iter()
                .map(|n| n.to_summary())
                .collect()
        }
    }

    fn search_notes_impl(self: &Arc<Self>, query: String) -> Vec<SearchResult> {
        if let Some(core) = &self.core {
            use aurora_core::traits::search_backend::SearchOptions;
            let search = core.search.clone();
            let result = self.runtime.block_on(async {
                search.search(&query, &SearchOptions::default()).await
            });

            match result {
                Ok(search_result) => search_result.hits.iter().map(|h| SearchResult {
                    note_id: h.note_id.clone(),
                    title: h.title.clone(),
                    snippet: h.snippet.clone(),
                    score: h.score as f64,
                }).collect(),
                Err(_) => {
                    // 搜索后端失败时，降级到简单的标题匹配
                    self.simple_title_search(&query)
                }
            }
        } else {
            self.simple_title_search(&query)
        }
    }

    fn simple_title_search(&self, query: &str) -> Vec<SearchResult> {
        let q_lower = query.to_lowercase();
        let notes = self.fallback_notes.lock().unwrap();
        notes
            .iter()
            .filter(|n| n.title.to_lowercase().contains(&q_lower))
            .map(|n| SearchResult {
                note_id: n.id.clone(),
                title: n.title.clone(),
                snippet: n.content.chars().take(100).collect(),
                score: 1.0,
            })
            .collect()
    }

    fn delete_note_impl(self: &Arc<Self>, note_id: String) -> Result<(), MobileError> {
        if let Some(core) = &self.core {
            let kv = core.kv_store.clone();
            let key = format!("note:{}", note_id);
            self.runtime.block_on(async {
                kv.delete(&key).await
            }).map_err(|e| MobileError::OperationFailed {
                message: format!("delete: {e}"),
            })?;

            // 从搜索索引中移除
            if let Some(search) = self.core.as_ref().map(|c| c.search.clone()) {
                let _ = self.runtime.block_on(async {
                    search.remove_index(&note_id).await
                });
            }
        } else {
            let mut notes = self.fallback_notes.lock().unwrap();
            notes.retain(|n| n.id != note_id);
        }
        Ok(())
    }

    /// V19 §36.3: saveNote(noteId, content) — 保存笔记内容
    fn save_note_content_impl(self: &Arc<Self>, note_id: String, content: String) -> Result<(), MobileError> {
        if let Some(core) = &self.core {
            let kv = core.kv_store.clone();
            let key = format!("note:{}", note_id);

            // 读取现有笔记
            let existing = self.runtime.block_on(async {
                kv.get(&key).await
            }).map_err(|e| MobileError::OperationFailed {
                message: format!("get: {e}"),
            })?;

            let mut note: NoteRecord = match existing {
                Some(bytes) => serde_json::from_slice(&bytes).map_err(|e| MobileError::OperationFailed {
                    message: format!("deserialize: {e}"),
                })?,
                None => return Err(MobileError::NotFound { resource: note_id }),
            };

            note.content = content;
            note.updated_at = chrono::Utc::now().to_rfc3339();

            let value = serde_json::to_vec(&note).map_err(|e| MobileError::OperationFailed {
                message: format!("serialize: {e}"),
            })?;

            self.runtime.block_on(async {
                kv.set(&key, &value).await
            }).map_err(|e| MobileError::OperationFailed {
                message: format!("set: {e}"),
            })?;

            // 更新搜索索引
            let search = core.search.clone();
            let note_clone = note.clone();
            self.runtime.block_on(async {
                use aurora_core::traits::search_backend::NoteMetadata;
                let metadata = NoteMetadata {
                    title: note_clone.title.clone(),
                    tags: vec![],
                    workspace_id: String::new(),
                    updated_at: Some(chrono::DateTime::parse_from_rfc3339(&note_clone.updated_at)
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .unwrap_or_else(|_| chrono::Utc::now())),
                };
                search.index_note(&note_clone.id, &note_clone.content, &metadata).await
            }).ok();
        } else {
            let mut notes = self.fallback_notes.lock().unwrap();
            for n in notes.iter_mut() {
                if n.id == note_id {
                    n.content = content;
                    n.updated_at = chrono::Utc::now().to_rfc3339();
                    return Ok(());
                }
            }
            return Err(MobileError::NotFound { resource: note_id });
        }
        Ok(())
    }

    /// V19 §36.3: getNoteContent(noteId) — 获取笔记内容
    fn get_note_content_impl(self: &Arc<Self>, note_id: String) -> Result<String, MobileError> {
        if let Some(core) = &self.core {
            let kv = core.kv_store.clone();
            let key = format!("note:{}", note_id);
            let result = self.runtime.block_on(async {
                kv.get(&key).await
            }).map_err(|e| MobileError::OperationFailed {
                message: format!("get: {e}"),
            })?;

            match result {
                Some(bytes) => {
                    let note: NoteRecord = serde_json::from_slice(&bytes).map_err(|e| MobileError::OperationFailed {
                        message: format!("deserialize: {e}"),
                    })?;
                    Ok(note.content)
                }
                None => Err(MobileError::NotFound { resource: note_id }),
            }
        } else {
            let notes = self.fallback_notes.lock().unwrap();
            for n in notes.iter() {
                if n.id == note_id {
                    return Ok(n.content.clone());
                }
            }
            Err(MobileError::NotFound { resource: note_id })
        }
    }
}

// ===========================================================================
// JNI 桥接层 — Android Java native 方法
// ===========================================================================

use jni::JNIEnv;
use jni::objects::{JClass, JString, JObject, JValue};
use jni::sys::{jlong, jint, jstring, jobject};

fn rust_str_to_jstring(env: &mut JNIEnv, s: &str) -> jstring {
    match env.new_string(s) {
        Ok(js) => js.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

fn jstring_to_rust(env: &mut JNIEnv, js: &JString) -> Option<String> {
    env.get_string(js).ok().map(|s| s.into())
}

unsafe fn core_from_handle(handle: jlong) -> Arc<UniffiAppCore> {
    let arc = Arc::from_raw(handle as *const UniffiAppCore);
    let cloned = Arc::clone(&arc);
    std::mem::forget(arc);
    cloned
}

#[no_mangle]
pub extern "system" fn Java_com_aurora_note_UniffiAppCore_nativeNew(
    mut env: JNIEnv,
    _class: JClass,
    data_dir: JString,
) -> jlong {
    let data_dir = match jstring_to_rust(&mut env, &data_dir) {
        Some(s) => s,
        None => return 0,
    };
    match UniffiAppCore::new(data_dir) {
        Ok(core) => Arc::into_raw(core) as jlong,
        Err(_) => 0,
    }
}

#[no_mangle]
pub extern "system" fn Java_com_aurora_note_UniffiAppCore_nativeCreateNote(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    title: JString,
) -> jstring {
    let core = unsafe { core_from_handle(handle) };
    let title = match jstring_to_rust(&mut env, &title) {
        Some(s) => s,
        None => return std::ptr::null_mut(),
    };
    match core.create_note_impl(title) {
        Ok(id) => rust_str_to_jstring(&mut env, &id),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "system" fn Java_com_aurora_note_UniffiAppCore_nativeListNotesCount(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jint {
    let core = unsafe { core_from_handle(handle) };
    core.list_notes_impl().len() as jint
}

#[no_mangle]
pub extern "system" fn Java_com_aurora_note_UniffiAppCore_nativeGetNote(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    index: jint,
) -> jobject {
    let core = unsafe { core_from_handle(handle) };
    let notes = core.list_notes_impl();
    if index < 0 || (index as usize) >= notes.len() {
        return std::ptr::null_mut();
    }
    let note = &notes[index as usize];

    let str_class = match env.find_class("java/lang/String") {
        Ok(c) => c,
        Err(_) => return std::ptr::null_mut(),
    };
    let array = match env.new_object_array(3, &str_class, JObject::null()) {
        Ok(a) => a,
        Err(_) => return std::ptr::null_mut(),
    };
    let id = rust_str_to_jstring(&mut env, &note.id);
    let _ = env.set_object_array_element(&array, 0, unsafe { JObject::from_raw(id) });
    let title = rust_str_to_jstring(&mut env, &note.title);
    let _ = env.set_object_array_element(&array, 1, unsafe { JObject::from_raw(title) });
    let updated = rust_str_to_jstring(&mut env, &note.updated_at);
    let _ = env.set_object_array_element(&array, 2, unsafe { JObject::from_raw(updated) });
    array.into_raw()
}

#[no_mangle]
pub extern "system" fn Java_com_aurora_note_UniffiAppCore_nativeSearchCount(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    query: JString,
) -> jint {
    let core = unsafe { core_from_handle(handle) };
    let query = match jstring_to_rust(&mut env, &query) {
        Some(s) => s,
        None => return 0,
    };
    core.search_notes_impl(query).len() as jint
}

#[no_mangle]
pub extern "system" fn Java_com_aurora_note_UniffiAppCore_nativeGetSearchResult(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    index: jint,
    query: JString,
) -> jobject {
    let core = unsafe { core_from_handle(handle) };
    let query = match jstring_to_rust(&mut env, &query) {
        Some(s) => s,
        None => return std::ptr::null_mut(),
    };
    let results = core.search_notes_impl(query);
    if index < 0 || (index as usize) >= results.len() {
        return std::ptr::null_mut();
    }
    let r = &results[index as usize];

    let obj_class = match env.find_class("java/lang/Object") {
        Ok(c) => c,
        Err(_) => return std::ptr::null_mut(),
    };
    let array = match env.new_object_array(4, &obj_class, JObject::null()) {
        Ok(a) => a,
        Err(_) => return std::ptr::null_mut(),
    };
    let note_id = rust_str_to_jstring(&mut env, &r.note_id);
    let _ = env.set_object_array_element(&array, 0, unsafe { JObject::from_raw(note_id) });
    let title = rust_str_to_jstring(&mut env, &r.title);
    let _ = env.set_object_array_element(&array, 1, unsafe { JObject::from_raw(title) });
    let snippet = rust_str_to_jstring(&mut env, &r.snippet);
    let _ = env.set_object_array_element(&array, 2, unsafe { JObject::from_raw(snippet) });

    let double_class = match env.find_class("java/lang/Double") {
        Ok(c) => c,
        Err(_) => return std::ptr::null_mut(),
    };
    let dv = match env.new_object(&double_class, "(D)V", &[JValue::Double(r.score)]) {
        Ok(d) => d,
        Err(_) => return std::ptr::null_mut(),
    };
    let _ = env.set_object_array_element(&array, 3, dv);
    array.into_raw()
}

#[no_mangle]
pub extern "system" fn Java_com_aurora_note_UniffiAppCore_nativeDeleteNote(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    note_id: JString,
) -> jint {
    let core = unsafe { core_from_handle(handle) };
    let note_id = match jstring_to_rust(&mut env, &note_id) {
        Some(s) => s,
        None => return -1,
    };
    match core.delete_note_impl(note_id) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

#[no_mangle]
pub extern "system" fn Java_com_aurora_note_UniffiAppCore_nativeIsFallback(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jint {
    let core = unsafe { core_from_handle(handle) };
    if core.is_fallback { 1 } else { 0 }
}

#[no_mangle]
pub extern "system" fn Java_com_aurora_note_UniffiAppCore_nativeSaveNoteContent(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    note_id: JString,
    content: JString,
) -> jint {
    let core = unsafe { core_from_handle(handle) };
    let note_id = match jstring_to_rust(&mut env, &note_id) {
        Some(s) => s,
        None => return -1,
    };
    let content = match jstring_to_rust(&mut env, &content) {
        Some(s) => s,
        None => return -1,
    };
    match core.save_note_content_impl(note_id, content) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

#[no_mangle]
pub extern "system" fn Java_com_aurora_note_UniffiAppCore_nativeGetNoteContent(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    note_id: JString,
) -> jstring {
    let core = unsafe { core_from_handle(handle) };
    let note_id = match jstring_to_rust(&mut env, &note_id) {
        Some(s) => s,
        None => return std::ptr::null_mut(),
    };
    match core.get_note_content_impl(note_id) {
        Ok(content) => rust_str_to_jstring(&mut env, &content),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "system" fn Java_com_aurora_note_UniffiAppCore_nativeDestroy(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) {
    if handle != 0 {
        unsafe {
            let _ = Arc::from_raw(handle as *const UniffiAppCore);
        }
    }
}

// ===========================================================================
// UniFFI export（保留用于 iOS / 测试）
// ===========================================================================

#[uniffi::export]
impl UniffiAppCore {
    #[uniffi::constructor]
    pub fn uniffi_new(data_dir: String) -> Result<Arc<Self>, MobileError> {
        Self::new(data_dir)
    }

    pub fn create_note(self: Arc<Self>, title: String) -> Result<String, MobileError> {
        Self::create_note_impl(&self, title)
    }

    pub fn list_notes(self: Arc<Self>) -> Vec<NoteSummary> {
        Self::list_notes_impl(&self)
    }

    pub fn search_notes(self: Arc<Self>, query: String) -> Vec<SearchResult> {
        Self::search_notes_impl(&self, query)
    }

    pub fn delete_note(self: Arc<Self>, note_id: String) -> Result<(), MobileError> {
        Self::delete_note_impl(&self, note_id)
    }

    pub fn save_note_content(self: Arc<Self>, note_id: String, content: String) -> Result<(), MobileError> {
        Self::save_note_content_impl(&self, note_id, content)
    }

    pub fn get_note_content(self: Arc<Self>, note_id: String) -> Result<String, MobileError> {
        Self::get_note_content_impl(&self, note_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_lifecycle() {
        let dir = tempfile::tempdir().unwrap();
        let core = UniffiAppCore::new(dir.path().to_str().unwrap().to_string()).unwrap();
        let id = core.clone().create_note("Test Note".into()).unwrap();
        let notes = core.list_notes();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].id, id);

        core.clone().delete_note(id).unwrap();
        assert!(core.list_notes().is_empty());
    }

    #[test]
    fn fallback_mode_works() {
        let core = UniffiAppCore::new("/dev/null/aurora-test".into()).unwrap();
        assert!(core.is_fallback);
        let id = core.clone().create_note("Fallback".into()).unwrap();
        assert_eq!(core.list_notes().len(), 1);
        core.clone().delete_note(id).unwrap();
        assert!(core.list_notes().is_empty());
    }
}
