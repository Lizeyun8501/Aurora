//! 附件存储（DK-09 request 裁决落地 · 2026-09-21）
//!
//! # 设计裁决（Alpha）
//! - **内容寻址去重**：数据本体键 `attachblob:{sha256}`（同内容一份）；
//!   元数据键 `attach:{id}`；反向索引 `attachnote:{note_id}:{id}`。
//! - **删除语义（修正 request 的"级联清理"）**：`delete` 只清 meta + 反向
//!   索引；blob 引用计数不做——内容寻址存储惯例：写时去重 + 孤儿 blob 由
//!   GC 切片回收，避免引用计数复杂度与"删 A 误伤 B"风险。
//! - **加密封装在编排层**（write_path::attach_to_note/read_attachment 走
//!   `WriteContext.seal` 字节封装）：ContentCipherPair 是 `(note_id, &str)`
//!   字符串接口，不适配二进制；附件与 vault DEK at-rest 同层，桌面密封 /
//!   移动明文降级语义一致。**store 本体纯存储（不感知加密）**。
//! - 完整性：read 时校验明文 sha256 == meta.sha256，fail-closed。

use crate::traits::kv_store::KVStore;
use crate::Error;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;

/// 附件元数据（KV JSON 权威格式）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AttachmentMeta {
    /// `at-` + uuid v4。
    pub attachment_id: String,
    pub note_id: String,
    pub file_name: String,
    pub mime: String,
    /// 明文字节数（用户语义的文件大小）。
    pub size: u64,
    /// hex 编码 sha256（**明文** hash——内容寻址与完整性校验基准）。
    pub sha256: String,
}

/// 附件存储抽象（纯存储；密封/解封在 write_path 编排层）。
#[async_trait]
pub trait AttachmentStore: Send + Sync {
    /// 写入（meta 由编排层构造：id/sha256/size 已定；sealed_data 为密封后
    /// 字节——明文模式即原字节）。同 sha256 blob 已存在则复用（去重）。
    async fn put(&self, meta: &AttachmentMeta, sealed_data: &[u8]) -> Result<(), Error>;

    async fn get_meta(&self, attachment_id: &str) -> Result<Option<AttachmentMeta>, Error>;

    /// 按内容寻址键取密封字节（GC/去重共用路径）。
    async fn get_blob(&self, sha256_hex: &str) -> Result<Option<Vec<u8>>, Error>;

    /// 笔记的全部附件（删除级联与导入报告用）。
    async fn list_by_note(&self, note_id: &str) -> Result<Vec<AttachmentMeta>, Error>;

    /// 删 meta + 反向索引（blob 留 GC）。
    async fn delete(&self, attachment_id: &str) -> Result<(), Error>;
}

// ===== KV 键（模块常量——GC/巡检工具复用同一约定）=====
pub const KEY_META: &str = "attach:";
pub const KEY_BLOB: &str = "attachblob:";
pub const KEY_NOTE_IDX: &str = "attachnote:";

pub fn meta_key(attachment_id: &str) -> String {
    format!("{KEY_META}{attachment_id}")
}
pub fn blob_key(sha256_hex: &str) -> String {
    format!("{KEY_BLOB}{sha256_hex}")
}
pub fn note_idx_key(note_id: &str, attachment_id: &str) -> String {
    format!("{KEY_NOTE_IDX}{note_id}:{attachment_id}")
}

pub fn new_attachment_id() -> String {
    format!("at-{}", uuid::Uuid::new_v4())
}

pub fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    crate::attachment_store::hex_encode(&h.finalize())
}

/// hex 编码（本地实现，避免引入 hex crate）。
pub fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0xf) as usize] as char);
    }
    out
}

// ===== 通用 KV 实现（桌面 SQLite KV / 移动同源共用）=====

pub struct KvAttachmentStore {
    kv: Arc<dyn KVStore>,
}

impl KvAttachmentStore {
    pub fn new(kv: Arc<dyn KVStore>) -> Self {
        Self { kv }
    }
}

#[async_trait]
impl AttachmentStore for KvAttachmentStore {
    async fn put(&self, meta: &AttachmentMeta, sealed_data: &[u8]) -> Result<(), Error> {
        let bk = blob_key(&meta.sha256);
        if !self.kv.exists(&bk).await? {
            self.kv.set(&bk, sealed_data).await?;
        }
        self.kv
            .set(
                &meta_key(&meta.attachment_id),
                serde_json::to_vec(meta)?.as_slice(),
            )
            .await?;
        self.kv
            .set(
                &note_idx_key(&meta.note_id, &meta.attachment_id),
                meta.attachment_id.as_bytes(),
            )
            .await?;
        Ok(())
    }

    async fn get_meta(&self, attachment_id: &str) -> Result<Option<AttachmentMeta>, Error> {
        match self.kv.get(&meta_key(attachment_id)).await? {
            Some(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            None => Ok(None),
        }
    }

    async fn get_blob(&self, sha256_hex: &str) -> Result<Option<Vec<u8>>, Error> {
        self.kv.get(&blob_key(sha256_hex)).await
    }

    async fn list_by_note(&self, note_id: &str) -> Result<Vec<AttachmentMeta>, Error> {
        let prefix = format!("{KEY_NOTE_IDX}{note_id}:");
        let pairs = self.kv.scan_prefix(&prefix).await?;
        let mut ids: Vec<&str> = pairs
            .iter()
            .map(|(_, v)| std::str::from_utf8(v).unwrap_or(""))
            .collect();
        ids.retain(|s| !s.is_empty());
        let keys: Vec<String> = ids.iter().map(|id| meta_key(id)).collect();
        let key_refs: Vec<&str> = keys.iter().map(String::as_str).collect();
        let metas = self.kv.batch_get(&key_refs).await?;
        let mut out = Vec::with_capacity(metas.len());
        for m in metas.into_iter().flatten() {
            out.push(serde_json::from_slice(&m)?);
        }
        Ok(out)
    }

    async fn delete(&self, attachment_id: &str) -> Result<(), Error> {
        if let Some(meta) = self.get_meta(attachment_id).await? {
            self.kv
                .delete(&note_idx_key(&meta.note_id, &meta.attachment_id))
                .await?;
            self.kv.delete(&meta_key(attachment_id)).await?;
        }
        Ok(())
    }
}

pub fn make_meta(note_id: &str, file_name: &str, mime: &str, plaintext: &[u8]) -> AttachmentMeta {
    AttachmentMeta {
        attachment_id: new_attachment_id(),
        note_id: note_id.to_string(),
        file_name: file_name.to_string(),
        mime: mime.to_string(),
        size: plaintext.len() as u64,
        sha256: sha256_hex(plaintext),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::l1_infrastructure::storage_engine::MemoryKVStore;

    fn store() -> KvAttachmentStore {
        KvAttachmentStore::new(Arc::new(MemoryKVStore::default()))
    }

    #[test]
    fn dk09_attach_put_get_roundtrip_dedup() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        rt.block_on(async {
            let s = store();
            let data = b"hello attachment".to_vec();
            let m1 = make_meta("n1", "a.png", "image/png", &data);
            s.put(&m1, &data).await.unwrap();
            // 同内容第二附件：blob 去重（blob 键只此一份）
            let m2 = make_meta("n2", "b.png", "image/png", &data);
            s.put(&m2, &data).await.unwrap();
            assert_ne!(m1.attachment_id, m2.attachment_id);
            assert_eq!(m1.sha256, m2.sha256);

            let got = s.get_meta(&m1.attachment_id).await.unwrap().unwrap();
            assert_eq!(got, m1);
            let blob = s.get_blob(&m1.sha256).await.unwrap().unwrap();
            assert_eq!(blob, data);
            assert_eq!(s.list_by_note("n1").await.unwrap().len(), 1);
            assert_eq!(s.list_by_note("n2").await.unwrap().len(), 1);
        });
    }

    #[test]
    fn dk09_attach_delete_cascade_index() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        rt.block_on(async {
            let s = store();
            let m = make_meta("n1", "a.bin", "app/octet-stream", b"x");
            s.put(&m, b"x").await.unwrap();
            assert!(s.get_meta(&m.attachment_id).await.unwrap().is_some());
            s.delete(&m.attachment_id).await.unwrap();
            assert!(s.get_meta(&m.attachment_id).await.unwrap().is_none());
            assert!(s.list_by_note("n1").await.unwrap().is_empty());
            // blob 留存（GC 回收）
            assert!(s.get_blob(&m.sha256).await.unwrap().is_some());
            s.delete(&m.attachment_id).await.unwrap(); // 幂等
        });
    }

    #[test]
    fn dk09_attach_get_unknown_none() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        rt.block_on(async {
            let s = store();
            assert!(s.get_meta("at-nope").await.unwrap().is_none());
            assert!(s.get_blob("deadbeef").await.unwrap().is_none());
        });
    }
}
