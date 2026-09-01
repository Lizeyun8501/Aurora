//! Aurora Note Mobile FFI (UniFFI + JNI)
//!
//! 对应 V19 §28.1 Trait 签名的移动端适配，提供跨平台 FFI 入口：
//! - iOS: 通过 UniFFI Record/Object 注解
//! - Android: 通过 JNI C ABI 桥接（Java_com_aurora_note_UniffiAppCore_*）
//!
//! 方法均为同步阻塞式（符合 V19 保留同步签名的决策）。
//! async Trait 方法（KVStore / SearchBackend）通过内部 tokio runtime 驱动。

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use aurora_core::l1_infrastructure::note_doc::NoteDoc;

uniffi::setup_scaffolding!();

// V19 §31 DEV-005: iroh P2P 同步引擎（feature-gated）
#[cfg(feature = "p2p-sync")]
pub mod p2p_sync;

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
// UniffiAppCore — 真实实现（接入 bootstrap + KVStore + SearchBackend + Loro）
// ===========================================================================

/// 每条笔记对应一个独立的 `NoteDoc`（V19 §30.1 五容器模型），
/// 快照持久化到 KVStore（`notesnap:{id}`），元数据存 `note:{id}`。
/// V19 §36.3: 移动端使用原生 Loro 绑定 — CRDT 语义，支持未来多端合并。
#[derive(uniffi::Object)]
pub struct UniffiAppCore {
    core: Option<Arc<aurora_core::app_core::AppCore>>,
    runtime: tokio::runtime::Runtime,
    data_dir: PathBuf,
    fallback_notes: Mutex<Vec<NoteRecord>>,
    /// Loro 文档缓存（note_id → NoteDoc 五容器模型）
    docs: Mutex<std::collections::HashMap<String, NoteDoc>>,
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
                tracing::info!("bootstrap success — full mode (loro CRDT enabled)");
                // V20 §4.5: 启动期投影追赶（restore_seq + catch_up 在
                // startup 后执行，杀进程后索引自动补齐）
                let core = booted.core.clone();
                runtime.block_on(async move {
                    let _ = core.startup();
                    let _ = core.catch_up_projections().await;
                });
                Ok(Arc::new(Self {
                    core: Some(booted.core),
                    runtime,
                    data_dir,
                    fallback_notes: Mutex::new(Vec::new()),
                    docs: Mutex::new(std::collections::HashMap::new()),
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
                    docs: Mutex::new(std::collections::HashMap::new()),
                    is_fallback: true,
                }))
            }
        }
    }

    /// 创建（或从 KVStore 恢复）笔记的 NoteDoc 并放入缓存。
    fn doc_for_note(&self, note_id: &str) -> NoteDoc {
        // 缓存命中
        if let Some(doc) = self.docs.lock().unwrap().get(note_id) {
            return doc.clone();
        }

        // 尝试从 KVStore 恢复快照
        let mut note_doc =
            NoteDoc::new("", "").unwrap_or_else(|_| NoteDoc::from_doc(loro::LoroDoc::new()));
        if let Some(core) = &self.core {
            let kv = core.kv_store.clone();
            let key = format!("notesnap:{note_id}");
            let snapshot = self
                .runtime
                .block_on(async { kv.get(&key).await.ok().flatten() });
            if let Some(bytes) = snapshot {
                if let Ok(d) = NoteDoc::from_snapshot(&bytes) {
                    note_doc = d;
                    tracing::debug!(note_id, "loro doc restored from snapshot");
                }
            }
        }

        self.docs
            .lock()
            .unwrap()
            .insert(note_id.to_string(), note_doc.clone());
        note_doc
    }

    /// 导出 Loro 快照并持久化到 KVStore。
    fn persist_doc(&self, note_id: &str, doc: &NoteDoc) -> Result<(), MobileError> {
        if let Some(core) = &self.core {
            let snapshot = doc
                .export_snapshot()
                .map_err(|e| MobileError::OperationFailed {
                    message: format!("loro export: {e}"),
                })?;
            let kv = core.kv_store.clone();
            let key = format!("notesnap:{note_id}");
            self.runtime
                .block_on(async { kv.set(&key, &snapshot).await })
                .map_err(|e| MobileError::OperationFailed {
                    message: format!("kv set snapshot: {e}"),
                })?;
        }
        Ok(())
    }

    /// P2P 同步用：公开访问指定笔记的 NoteDoc（缓存/快照恢复）。
    #[cfg(feature = "p2p-sync")]
    pub fn doc_for_note_public(&self, note_id: &str) -> Result<NoteDoc, MobileError> {
        // 验证笔记存在（与 get_note_content 一致的语义）
        let exists = if let Some(core) = &self.core {
            let kv = core.kv_store.clone();
            let key = format!("note:{note_id}");
            self.runtime
                .block_on(async { kv.get(&key).await })
                .map_err(|e| MobileError::OperationFailed {
                    message: format!("get: {e}"),
                })?
                .is_some()
        } else {
            self.fallback_notes
                .lock()
                .unwrap()
                .iter()
                .any(|n| n.id == note_id)
        };
        if !exists {
            return Err(MobileError::NotFound {
                resource: note_id.to_string(),
            });
        }
        Ok(self.doc_for_note(note_id))
    }

    /// P2P 同步用：公开持久化指定笔记当前缓存快照。
    #[cfg(feature = "p2p-sync")]
    pub fn persist_note_snapshot(&self, note_id: &str) -> Result<(), MobileError> {
        let doc = self.doc_for_note(note_id);
        self.persist_doc(note_id, &doc)
    }

    fn create_note_impl(self: &Arc<Self>, title: String) -> Result<String, MobileError> {
        let note = NoteRecord::new(title);

        // 创建五容器 Loro 文档（meta/body/blocks/tasks/backlinks）
        let now_ms = chrono::Utc::now().timestamp_millis();
        let doc = NoteDoc::new(&note.title, "").map_err(|e| MobileError::OperationFailed {
            message: format!("note doc init: {e}"),
        })?;
        doc.set_timestamps(now_ms, now_ms)
            .map_err(|e| MobileError::OperationFailed {
                message: format!("note doc timestamps: {e}"),
            })?;

        if let Some(core) = &self.core {
            // 真实模式：KVStore 持久化 + SearchBackend 索引
            let kv = core.kv_store.clone();
            let key = format!("note:{}", note.id);
            let value = serde_json::to_vec(&note).map_err(|e| MobileError::OperationFailed {
                message: format!("serialize: {e}"),
            })?;

            self.runtime
                .block_on(async { kv.set(&key, &value).await })
                .map_err(|e| MobileError::OperationFailed {
                    message: format!("kv set: {e}"),
                })?;

            // 持久化 Loro 快照
            self.persist_doc(&note.id, &doc)?;

            // V20 §4.5 事件驱动: 发 NoteCreated 事件（投影消费建索引，
            // 替代直接 index_note 旁路 — 保证与重建路径同源一致）
            core.event_bus.publish(
                aurora_core::event_bus::layered::AppEvent::NoteCreated {
                    note_id: note.id.clone(),
                    title: note.title.clone(),
                    content: note.content.clone(),
                },
            );
            // 同步驱动投影追赶（移动端单线程 runtime，启动期/写后各一次）
            let core_clone = core.clone();
            self.runtime
                .block_on(async move { core_clone.catch_up_projections().await })
                .ok();

            // 缓存 LoroDoc
            self.docs.lock().unwrap().insert(note.id.clone(), doc);
        } else {
            // Fallback 模式：内存存储（LoroDoc 仍然提供 CRDT 语义）
            self.docs.lock().unwrap().insert(note.id.clone(), doc);
            self.fallback_notes.lock().unwrap().push(note.clone());
        }

        Ok(note.id)
    }

    fn list_notes_impl(self: &Arc<Self>) -> Vec<NoteSummary> {
        if let Some(core) = &self.core {
            let kv = core.kv_store.clone();
            let entries = self
                .runtime
                .block_on(async { kv.scan_prefix("note:").await });

            match entries {
                Ok(pairs) => pairs
                    .iter()
                    .filter_map(|(_, bytes)| serde_json::from_slice::<NoteRecord>(bytes).ok())
                    .map(|n| n.to_summary())
                    .collect(),
                Err(_) => Vec::new(),
            }
        } else {
            self.fallback_notes
                .lock()
                .unwrap()
                .iter()
                .map(|n| n.to_summary())
                .collect()
        }
    }

    fn search_notes_impl(self: &Arc<Self>, query: String) -> Vec<SearchResult> {
        if let Some(core) = &self.core {
            use aurora_core::traits::search_backend::SearchOptions;
            let search = core.search.clone();
            let result = self
                .runtime
                .block_on(async { search.search(&query, &SearchOptions::default()).await });

            match result {
                Ok(search_result) => search_result
                    .hits
                    .iter()
                    .map(|h| SearchResult {
                        note_id: h.note_id.clone(),
                        title: h.title.clone(),
                        snippet: h.snippet.clone(),
                        score: h.score as f64,
                    })
                    .collect(),
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
            self.runtime
                .block_on(async { kv.delete(&key).await })
                .map_err(|e| MobileError::OperationFailed {
                    message: format!("delete: {e}"),
                })?;

            // V20 §4.5 事件驱动: NoteDeleted（投影删索引）+ 同步追赶
            use aurora_core::event_bus::layered::AppEvent;
            core.event_bus.publish(AppEvent::NoteDeleted {
                note_id: note_id.clone(),
            });
            let core_clone = core.clone();
            self.runtime
                .block_on(async move { core_clone.catch_up_projections().await })
                .ok();
            // 清理 Loro 文档缓存
            self.docs.lock().unwrap().remove(&note_id);
        } else {
            let mut notes = self.fallback_notes.lock().unwrap();
            notes.retain(|n| n.id != note_id);
        }
        Ok(())
    }

    /// V19 §36.3: saveNote(noteId, content) — 保存笔记内容（Loro CRDT）
    fn save_note_content_impl(
        self: &Arc<Self>,
        note_id: String,
        content: String,
    ) -> Result<(), MobileError> {
        // 通过五容器模型的 body 容器写入（CRDT 语义：可多端合并）
        let doc = self.doc_for_note(&note_id);
        let now_ms = chrono::Utc::now().timestamp_millis();
        doc.set_body(&content, now_ms)
            .map_err(|e| MobileError::OperationFailed {
                message: format!("loro set_body: {e}"),
            })?;

        // 持久化 Loro 快照
        self.persist_doc(&note_id, &doc)?;

        if let Some(core) = &self.core {
            let kv = core.kv_store.clone();
            let key = format!("note:{}", note_id);

            // 同步元数据 JSON（updated_at）
            let existing = self
                .runtime
                .block_on(async { kv.get(&key).await })
                .map_err(|e| MobileError::OperationFailed {
                    message: format!("get: {e}"),
                })?;

            let mut note: NoteRecord = match existing {
                Some(bytes) => {
                    serde_json::from_slice(&bytes).map_err(|e| MobileError::OperationFailed {
                        message: format!("deserialize: {e}"),
                    })?
                }
                None => return Err(MobileError::NotFound { resource: note_id }),
            };

            note.content = content;
            note.updated_at = chrono::Utc::now().to_rfc3339();

            let value = serde_json::to_vec(&note).map_err(|e| MobileError::OperationFailed {
                message: format!("serialize: {e}"),
            })?;

            self.runtime
                .block_on(async { kv.set(&key, &value).await })
                .map_err(|e| MobileError::OperationFailed {
                    message: format!("set: {e}"),
                })?;

            // V20 §4.5 事件驱动: 内容变更 → NoteMetadataChanged（投影从
            // KVStore 数据源重取最新内容重建索引，写路径与 rebuild 同源）
            use aurora_core::event_bus::layered::{AppEvent, NoteChanges};
            core.event_bus.publish(AppEvent::NoteMetadataChanged {
                note_id: note.id.clone(),
                changes: NoteChanges {
                    title: None,
                    tags: None,
                },
            });
            let core_clone = core.clone();
            self.runtime
                .block_on(async move { core_clone.catch_up_projections().await })
                .ok();
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

    /// V19 §36.3: getNoteContent(noteId) — 获取笔记内容（从 LoroText 读取）
    fn get_note_content_impl(self: &Arc<Self>, note_id: String) -> Result<String, MobileError> {
        // 验证笔记存在（元数据）
        let exists = if let Some(core) = &self.core {
            let kv = core.kv_store.clone();
            let key = format!("note:{}", note_id);
            let result = self
                .runtime
                .block_on(async { kv.get(&key).await })
                .map_err(|e| MobileError::OperationFailed {
                    message: format!("get: {e}"),
                })?;
            result.is_some()
        } else {
            let notes = self.fallback_notes.lock().unwrap();
            notes.iter().any(|n| n.id == note_id)
        };

        if !exists {
            return Err(MobileError::NotFound { resource: note_id });
        }

        // 从五容器模型读取正文 body（缓存命中或从快照恢复）
        let doc = self.doc_for_note(&note_id);
        Ok(doc.body())
    }

    /// 导出笔记的完整 Loro 快照（base64）— ProseMirror 编辑器初始化用（DEV-009）。
    ///
    /// 返回 Rust 侧 NoteDoc 当前状态的快照（含 P2P 合并结果）。
    fn get_note_snapshot_impl(self: &Arc<Self>, note_id: String) -> Result<String, MobileError> {
        let exists = if let Some(core) = &self.core {
            let kv = core.kv_store.clone();
            let key = format!("note:{}", note_id);
            self.runtime
                .block_on(async { kv.get(&key).await })
                .map_err(|e| MobileError::OperationFailed {
                    message: format!("get: {e}"),
                })?
                .is_some()
        } else {
            self.fallback_notes
                .lock()
                .unwrap()
                .iter()
                .any(|n| n.id == note_id)
        };
        if !exists {
            return Err(MobileError::NotFound { resource: note_id });
        }

        let doc = self.doc_for_note(&note_id);
        let snapshot = doc
            .export_snapshot()
            .map_err(|e| MobileError::OperationFailed {
                message: format!("loro export: {e}"),
            })?;
        Ok(base64_encode(&snapshot))
    }

    /// 将 JS 侧 Loro 快照（base64）合并进 Rust 侧 NoteDoc 并持久化（DEV-009）。
    ///
    /// 合并语义（CRDT）: import 合并而非替换 — P2P 对端修改不丢失。
    fn save_note_snapshot_impl(
        self: &Arc<Self>,
        note_id: String,
        snapshot_b64: String,
    ) -> Result<(), MobileError> {
        let bytes = base64_decode(&snapshot_b64).ok_or_else(|| MobileError::OperationFailed {
            message: "invalid base64 snapshot".into(),
        })?;
        if bytes.is_empty() {
            return Ok(()); // 空快照视为无操作
        }

        // 合并进缓存文档（快照 blob 与 update 均可 import）
        let doc = self.doc_for_note(&note_id);
        doc.apply_update(&bytes)
            .map_err(|e| MobileError::OperationFailed {
                message: format!("loro import: {e}"),
            })?;

        // 持久化
        self.persist_doc(&note_id, &doc)
    }
}

// ===========================================================================
// JNI 桥接层 — Android Java native 方法
// ===========================================================================

use jni::objects::{JClass, JObject, JString, JValue};
use jni::sys::{jboolean, jint, jlong, jobject, jstring};
use jni::JNIEnv;

fn rust_str_to_jstring(env: &mut JNIEnv, s: &str) -> jstring {
    match env.new_string(s) {
        Ok(js) => js.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

fn jstring_to_rust(env: &mut JNIEnv, js: &JString) -> Option<String> {
    env.get_string(js).ok().map(|s| s.into())
}

/// base64 编码（标准字母表，无换行）— Loro 快照跨 JNI 传输。
fn base64_encode(input: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(CHARS[(n >> 18) as usize & 63] as char);
        out.push(CHARS[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            CHARS[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            CHARS[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// base64 解码（容忍空白字符）。失败返回 None。
fn base64_decode(input: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some((c - b'A') as u32),
            b'a'..=b'z' => Some((c - b'a' + 26) as u32),
            b'0'..=b'9' => Some((c - b'0' + 52) as u32),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let mut out = Vec::with_capacity(input.len() / 4 * 3);
    let mut buf: u32 = 0;
    let mut bits = 0u32;
    for &c in input.as_bytes() {
        if c.is_ascii_whitespace() || c == b'=' {
            continue;
        }
        let v = val(c)?;
        buf = (buf << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    Some(out)
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
    if core.is_fallback {
        1
    } else {
        0
    }
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

/// 获取笔记 Loro 快照（base64）— ProseMirror 编辑器初始化（DEV-009）。
#[no_mangle]
pub extern "system" fn Java_com_aurora_note_UniffiAppCore_nativeGetNoteSnapshot(
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
    match core.get_note_snapshot_impl(note_id) {
        Ok(b64) => rust_str_to_jstring(&mut env, &b64),
        Err(_) => std::ptr::null_mut(),
    }
}

/// 保存 JS 侧 Loro 快照（base64，CRDT 合并）+ 持久化（DEV-009）。
#[no_mangle]
pub extern "system" fn Java_com_aurora_note_UniffiAppCore_nativeSaveNoteSnapshot(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    note_id: JString,
    snapshot_b64: JString,
) -> jboolean {
    let core = unsafe { core_from_handle(handle) };
    let (Some(note_id), Some(snapshot_b64)) = (
        jstring_to_rust(&mut env, &note_id),
        jstring_to_rust(&mut env, &snapshot_b64),
    ) else {
        return 0;
    };
    match core.save_note_snapshot_impl(note_id, snapshot_b64) {
        Ok(()) => 1,
        Err(e) => {
            tracing::warn!("save note snapshot failed: {e}");
            0
        }
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

    pub fn save_note_content(
        self: Arc<Self>,
        note_id: String,
        content: String,
    ) -> Result<(), MobileError> {
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
        let notes = core.clone().list_notes();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].id, id);

        // Loro 内容读写（真实模式）
        core.clone()
            .save_note_content(id.clone(), "Hello Loro 世界".into())
            .unwrap();
        let content = core.clone().get_note_content(id.clone()).unwrap();
        assert_eq!(content, "Hello Loro 世界");

        // 再写一次（覆盖路径）
        core.clone()
            .save_note_content(id.clone(), "Updated".into())
            .unwrap();
        assert_eq!(
            core.clone().get_note_content(id.clone()).unwrap(),
            "Updated"
        );

        core.clone().delete_note(id).unwrap();
        assert!(core.clone().list_notes().is_empty());
    }

    /// V20 §4.5 事件驱动索引闭环: 创建/搜索/删除全链路经投影。
    #[test]
    fn event_driven_search_index_lifecycle() {
        let dir = tempfile::tempdir().unwrap();
        let core = UniffiAppCore::new(dir.path().to_str().unwrap().to_string()).unwrap();
        assert!(!core.is_fallback);

        // 创建 → 事件 → 投影 → 可搜（中文 jieba）
        let id = core.clone().create_note("架构设计文档".into()).unwrap();
        let hits = core.clone().search_notes("架构".into());
        assert_eq!(hits.len(), 1, "创建后立即可搜（事件驱动投影）: {hits:?}");
        assert_eq!(hits[0].note_id, id);

        // 内容更新 → NoteMetadataChanged → 数据源重索引 → 可搜新内容
        core.clone()
            .save_note_content(id.clone(), "量子加密同步协议".into())
            .unwrap();
        let hits = core.clone().search_notes("量子".into());
        assert_eq!(hits.len(), 1, "内容更新后可搜新内容: {hits:?}");

        // 删除 → NoteDeleted → 索引清除
        core.clone().delete_note(id).unwrap();
        let hits = core.clone().search_notes("架构".into());
        assert!(hits.is_empty(), "删除后不再命中");
    }

    /// V20 §4.5 重建自愈: 全新实例从 KVStore 数据源全量重建索引。
    #[test]
    fn rebuild_from_data_source_on_new_instance() {
        let dir = tempfile::tempdir().unwrap();
        {
            let core = UniffiAppCore::new(dir.path().to_str().unwrap().to_string()).unwrap();
            core.clone().create_note("分布式系统笔记".into()).unwrap();
            core.clone().create_note("算法导论笔记".into()).unwrap();
        } // drop = 杀进程（Tantivy 索引仍在磁盘，事件队列仍在）

        // 新实例: startup catch_up 增量追赶（索引已在）
        let core2 = UniffiAppCore::new(dir.path().to_str().unwrap().to_string()).unwrap();
        let hits = core2.clone().search_notes("分布式".into());
        assert_eq!(hits.len(), 1, "重启后中文搜索可用: {hits:?}");

        // 模拟索引损坏: 新建第三个实例（KVStore 有数据）→ 删索引目录 →
        // rebuild 路径由 verify 失败触发（此处验证数据源回调可重建）
        drop(core2);
        let index_dir = dir.path().join("tantivy_index");
        let _ = std::fs::remove_dir_all(index_dir);
        let core3 = UniffiAppCore::new(dir.path().to_str().unwrap().to_string()).unwrap();
        // 索引目录被删 → tantivy 新建空索引; 事件队列 watermark 已消费 →
        // 增量追赶无事件可放 → 需走 rebuild（verify Corrupted → 数据源重建）。
        // 由于 verify 需查 doc 数对比，此处直接调 catch_up 验证不 panic，
        // 并手动触发数据源重建路径（rebuild_index 经 source 回调）。
        let hits = core3.clone().search_notes("算法".into());
        // 数据源回调重建后应命中（若 verify 未触发，此断言暴露重建缺口）
        assert_eq!(hits.len(), 1, "索引损坏后经数据源重建恢复: {hits:?}");
    }

    #[test]
    fn fallback_mode_works() {
        let core = UniffiAppCore::new("/dev/null/aurora-test".into()).unwrap();
        assert!(core.is_fallback);
        let id = core.clone().create_note("Fallback".into()).unwrap();
        assert_eq!(core.clone().list_notes().len(), 1);

        // Fallback 模式下 Loro 内存 CRDT 同样可用
        core.clone()
            .save_note_content(id.clone(), "fallback content".into())
            .unwrap();
        assert_eq!(
            core.clone().get_note_content(id.clone()).unwrap(),
            "fallback content"
        );

        core.clone().delete_note(id).unwrap();
        assert!(core.clone().list_notes().is_empty());
    }
}
