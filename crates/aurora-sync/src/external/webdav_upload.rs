//! WebDAV 大文件分片上传与断点续传 (Chunked Upload / Resumable Transfer)
//! — DK-08 第三切片（任务书 §7.1，DK-08 任务 5）。
//!
//! WebDAV 的 PUT 不可追加（`Content-Range` 为非标扩展），可移植的分片方案：
//!
//! ```text
//! {base}/aurora/{doc_id}.part-00000      — 分片 i（顺序 PUT，可乱序重试）
//! {base}/aurora/{doc_id}.part-00001
//! ...
//! {base}/aurora/{doc_id}.manifest.json   — 提交点：{doc_id,total_len,chunk_size,parts,etags}
//! ```
//!
//! # 断点续传模型
//!
//! [`UploadSession`] 是唯一事实源（可 serde 持久化到 offline_queue/SQLite）：
//! 记录 `committed_parts`（已确认 PUT 成功的分片数）与各分片 ETag。
//! [`ChunkedUploader::upload`] 从 `committed_parts` 继续推进 —— **已提交分片
//! 永不重传**（测试以请求次数断言证明）。每次分片成功后立即推进会话，
//! 因此调用中途失败（网络断/401）也会保留全部已完成进度。
//!
//! # 语义边界（有意为之）
//!
//! - 上传器是**纯传输层**：manifest 提交 ≠ 版本推进。大文档的 index 版本
//!   推进仍由调用方通过 `push_with_merge` / `send_update` 组合完成；
//! - 接收侧用 [`ChunkedUploader::download_reassembled`] 取回完整字节
//!   （GET manifest → 逐分片 GET → 拼接 + 长度校验）；
//! - manifest 缺失 → `Err(NotFound)`（fail-closed，不猜测部分状态）。

use aurora_core::traits::sync_target::Connection;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use super::webdav::WebDavTarget;

/// 默认分片大小（1 MiB）。
pub const DEFAULT_CHUNK_SIZE: usize = 1 << 20;

/// 分片最小尺寸钳制（防止退化成逐字节上传）。
pub const MIN_CHUNK_SIZE: usize = 64;

/// 分片/manifest 文件路径前缀约定。
const AURORA_DIR: &str = "aurora";

/// 分片上传会话状态 —— 断点续传的唯一事实源，可持久化。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UploadSession {
    pub doc_id: String,
    /// 负载总长（与会话绑定的负载指纹；payload.len() 不符即拒绝）。
    pub total_len: u64,
    pub chunk_size: usize,
    /// 已确认 PUT 成功的分片数（按序推进；断点）。
    pub committed_parts: u32,
    /// 各分片 ETag（PUT 响应回填；长度 == 分片总数，未上传为 None）。
    pub etags: Vec<Option<String>>,
}

impl UploadSession {
    /// 新建会话（`chunk_size` 钳制到 [`MIN_CHUNK_SIZE`] 下限）。
    pub fn new(doc_id: impl Into<String>, total_len: u64, chunk_size: usize) -> Self {
        let chunk_size = chunk_size.max(MIN_CHUNK_SIZE);
        Self {
            doc_id: doc_id.into(),
            total_len,
            chunk_size,
            committed_parts: 0,
            etags: Vec::new(),
        }
    }

    /// 分片总数（空负载 = 0）。
    pub fn total_parts(&self) -> u32 {
        if self.total_len == 0 {
            0
        } else {
            self.total_len.div_ceil(self.chunk_size as u64) as u32
        }
    }

    /// 全部分片已提交（manifest 提交前的本地判定）。
    pub fn is_complete(&self) -> bool {
        self.committed_parts >= self.total_parts() && self.total_parts() > 0
    }
}

/// manifest 提交点内容（`{doc_id}.manifest.json`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadManifest {
    pub doc_id: String,
    pub total_len: u64,
    pub chunk_size: usize,
    pub parts: u32,
    pub etags: Vec<Option<String>>,
}

/// 单次 `upload` 调用的进展。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UploadProgress {
    /// 还有分片未传（本次调到 `max_parts_per_call` 上限或部分失败前中断）。
    Partial { committed: u32, total: u32 },
    /// 全部分片完成且 manifest 已提交。
    Completed { manifest_path: String },
}

/// 分片上传器 —— 组合 [`WebDavTarget`] 的传输原语。
pub struct ChunkedUploader<'a> {
    target: &'a WebDavTarget,
    conn: &'a Connection,
    /// 单次 `upload` 调用最多 PUT 的分片数（限流/测试断点用；≥1）。
    max_parts_per_call: u32,
}

impl<'a> ChunkedUploader<'a> {
    pub fn new(target: &'a WebDavTarget, conn: &'a Connection) -> Self {
        Self {
            target,
            conn,
            max_parts_per_call: u32::MAX,
        }
    }

    /// 限流构造：单次调用最多 `max_parts_per_call` 个分片 PUT。
    ///
    /// 返回 `Err(InvalidInput)` 若为 0（无意义的空转调用）。
    pub fn with_rate_limit(
        target: &'a WebDavTarget,
        conn: &'a Connection,
        max_parts_per_call: u32,
    ) -> Result<Self, aurora_core::Error> {
        if max_parts_per_call == 0 {
            return Err(aurora_core::Error::InvalidInput(
                "max_parts_per_call must be >= 1".into(),
            ));
        }
        Ok(Self {
            target,
            conn,
            max_parts_per_call,
        })
    }

    fn part_path(doc_id: &str, index: u32) -> String {
        format!(
            "/{AURORA_DIR}/{}.part-{index:05}",
            super::webdav::encode_doc_id(doc_id)
        )
    }

    fn manifest_path(doc_id: &str) -> String {
        format!(
            "/{AURORA_DIR}/{}.manifest.json",
            super::webdav::encode_doc_id(doc_id)
        )
    }

    /// 开始/续传分片上传。
    ///
    /// - 从 `session.committed_parts` 继续，已提交分片**不重传**；
    /// - 每个分片 PUT 成功后立刻推进会话（中断零进度丢失）；
    /// - 全部分片完成 → PUT manifest（提交点）→ `Completed`；
    /// - 会话与负载不匹配（total_len 变了 / committed 越界）→
    ///   `Err(InvalidInput)`，零 HTTP 请求。
    pub async fn upload(
        &self,
        session: &mut UploadSession,
        payload: &[u8],
    ) -> Result<UploadProgress, aurora_core::Error> {
        self.validate(session, payload)?;

        let total = session.total_parts();
        let budget = self.max_parts_per_call;
        let mut uploaded_this_call = 0u32;

        while session.committed_parts < total && uploaded_this_call < budget {
            let i = session.committed_parts;
            let start = i as usize * session.chunk_size;
            let end = ((i as usize + 1) * session.chunk_size).min(payload.len());
            let chunk = &payload[start..end];

            let etag = self
                .target
                .put_file(self.conn, &Self::part_path(&session.doc_id, i), chunk)
                .await?;
            session.etags.push(etag);
            session.committed_parts += 1;
            uploaded_this_call += 1;
            debug!(
                doc_id = %session.doc_id,
                part = i,
                committed = session.committed_parts,
                total,
                "chunk uploaded"
            );
        }

        if session.committed_parts < total {
            return Ok(UploadProgress::Partial {
                committed: session.committed_parts,
                total,
            });
        }

        // 提交点：manifest（空负载 committed==0==total 也走到这里）
        let manifest = UploadManifest {
            doc_id: session.doc_id.clone(),
            total_len: session.total_len,
            chunk_size: session.chunk_size,
            parts: total,
            etags: session.etags.clone(),
        };
        let body = serde_json::to_vec(&manifest).map_err(aurora_core::Error::Serialization)?;
        let path = Self::manifest_path(&session.doc_id);
        self.target.put_file(self.conn, &path, &body).await?;
        info!(doc_id = %session.doc_id, parts = total, total_len = session.total_len, "chunked upload committed");
        Ok(UploadProgress::Completed {
            manifest_path: path,
        })
    }

    /// 接收侧：按 manifest 取回并拼装完整字节。
    ///
    /// manifest 缺失 → `Err(NotFound)`；分片缺失 / 拼装长度与
    /// `total_len` 不符 → `Err(Network)`（远端不一致，fail-closed）。
    pub async fn download_reassembled(&self, doc_id: &str) -> Result<Vec<u8>, aurora_core::Error> {
        let path = Self::manifest_path(doc_id);
        let body = self.target.get_file(self.conn, &path).await?;
        let manifest: UploadManifest =
            serde_json::from_slice(&body).map_err(aurora_core::Error::Serialization)?;

        let mut out = Vec::with_capacity(manifest.total_len as usize);
        for i in 0..manifest.parts {
            let chunk = self
                .target
                .get_file(self.conn, &Self::part_path(doc_id, i))
                .await?;
            out.extend_from_slice(&chunk);
        }
        if out.len() as u64 != manifest.total_len {
            return Err(aurora_core::Error::Network(format!(
                "chunked reassembly size mismatch: got {}, manifest {}",
                out.len(),
                manifest.total_len
            )));
        }
        debug!(
            doc_id,
            parts = manifest.parts,
            bytes = out.len(),
            "chunked reassembled"
        );
        Ok(out)
    }

    /// 会话/负载一致性守卫（零 HTTP）。
    fn validate(&self, session: &UploadSession, payload: &[u8]) -> Result<(), aurora_core::Error> {
        if payload.len() as u64 != session.total_len {
            return Err(aurora_core::Error::InvalidInput(format!(
                "payload len {} != session total_len {} (stale session?)",
                payload.len(),
                session.total_len
            )));
        }
        if session.committed_parts > session.total_parts() {
            return Err(aurora_core::Error::InvalidInput(format!(
                "committed_parts {} > total_parts {} (stale session?)",
                session.committed_parts,
                session.total_parts()
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aurora_core::traits::sync_target::{Endpoint, SyncProtocol, SyncTarget};
    use mockito::{Matcher, Server};

    fn endpoint_for(server: &Server) -> Endpoint {
        Endpoint {
            url: server.url(),
            protocol: SyncProtocol::WebSocket,
            auth: None,
        }
    }

    async fn connect_to(server: &Server, target: &mut WebDavTarget) -> Connection {
        let ep = endpoint_for(server);
        target.connect(&ep).await.unwrap()
    }

    /// 确定性 ASCII 负载（可安全用于 match_body::Exact）。
    fn test_payload(total: usize) -> Vec<u8> {
        (0..total).map(|i| b'0' + ((i * 7) % 10) as u8).collect()
    }

    /// S3 验收 1：分片上传 + 中断 + 断点续传 + roundtrip。
    ///
    /// 核心证据（请求次数断言）：限流 2 片/次 → 首调只传 part0/part1；
    /// 续传（会话经 serde 持久化还原）**只传 part2..4**，part0/part1 计数
    /// 保持 1（不重传）；manifest 仅提交一次；读侧拼装字节与原负载一致。
    #[tokio::test]
    async fn dk08_chunk_upload_roundtrip_with_resume() {
        let payload = test_payload(320); // 5 片 × 64B
        let mut session = UploadSession::new("big-note", payload.len() as u64, 64);
        assert_eq!(session.total_parts(), 5);

        let mut server = Server::new_async().await;
        let part_mocks = (0..5u32)
            .map(|i| {
                server
                    .mock("PUT", format!("/aurora/big-note.part-{i:05}").as_str())
                    .with_status(201)
                    .expect(1) // 全程恰好一次 —— 续传不重传的硬证据
                    .create()
            })
            .collect::<Vec<_>>();
        let manifest_mock = server
            .mock("PUT", "/aurora/big-note.manifest.json")
            .match_body(Matcher::Regex("\"parts\":5".into()))
            .with_status(201)
            .expect(1)
            .create();

        let mut target = WebDavTarget::new();
        let conn = connect_to(&server, &mut target).await;

        // ── 第一次：限流 2 片 → Partial，随后「断点」持久化 ──
        let uploader = ChunkedUploader::with_rate_limit(&target, &conn, 2).unwrap();
        let progress = uploader.upload(&mut session, &payload).await.unwrap();
        assert_eq!(
            progress,
            UploadProgress::Partial {
                committed: 2,
                total: 5
            }
        );
        let checkpoint = serde_json::to_vec(&session).unwrap(); // 断点落盘

        // ── 第二次：从持久化断点恢复，传完剩余 3 片 + manifest ──
        let mut restored: UploadSession = serde_json::from_slice(&checkpoint).unwrap();
        let uploader2 = ChunkedUploader::new(&target, &conn);
        let progress = uploader2.upload(&mut restored, &payload).await.unwrap();
        assert_eq!(
            progress,
            UploadProgress::Completed {
                manifest_path: "/aurora/big-note.manifest.json".into()
            }
        );
        assert!(restored.is_complete());
        assert_eq!(restored.committed_parts, 5);

        for m in &part_mocks {
            m.assert(); // 每片恰好 1 次 PUT（0/1 号片未因续传而重复）
        }
        manifest_mock.assert();

        // ── 读侧拼装：另一台设备按 manifest 重组 ──
        // 复用同一 stub server：补 GET mocks（manifest + 5 分片）
        let get_manifest = server
            .mock("GET", "/aurora/big-note.manifest.json")
            .with_status(200)
            .with_body(
                serde_json::to_vec(&UploadManifest {
                    doc_id: "big-note".into(),
                    total_len: payload.len() as u64,
                    chunk_size: 64,
                    parts: 5,
                    etags: vec![None; 5],
                })
                .unwrap(),
            )
            .expect(1)
            .create();
        let get_parts = (0..5u32)
            .map(|i| {
                server
                    .mock("GET", format!("/aurora/big-note.part-{i:05}").as_str())
                    .with_status(200)
                    .with_body(&payload[i as usize * 64..(i as usize + 1) * 64])
                    .expect(1)
                    .create()
            })
            .collect::<Vec<_>>();
        let mut reader_target = WebDavTarget::new();
        let reader_conn = connect_to(&server, &mut reader_target).await;
        let reader = ChunkedUploader::new(&reader_target, &reader_conn);
        let assembled = reader.download_reassembled("big-note").await.unwrap();
        assert_eq!(assembled, payload, "拼装字节必须与原负载一致");
        get_manifest.assert();
        for m in &get_parts {
            m.assert();
        }
    }

    /// S3 验收 2：会话与负载不匹配 → InvalidInput，零 HTTP 请求。
    #[tokio::test]
    async fn dk08_chunk_size_mismatch_rejected() {
        let mut server = Server::new_async().await;
        let any_put = server.mock("PUT", Matcher::Any).expect(0).create();
        let mut target = WebDavTarget::new();
        let conn = connect_to(&server, &mut target).await;

        let session = UploadSession::new("note", 320, 64);
        let uploader = ChunkedUploader::new(&target, &conn);
        let err = uploader
            .upload(&mut session.clone(), &test_payload(100))
            .await
            .unwrap_err();
        assert!(
            matches!(err, aurora_core::Error::InvalidInput(_)),
            "长度不符必须拒绝，得到 {err:?}"
        );

        // committed 越界（过期会话）同样拒绝
        let mut stale = UploadSession::new("note", 320, 64);
        stale.committed_parts = 9; // total_parts = 5
        let err = uploader
            .upload(&mut stale, &test_payload(320))
            .await
            .unwrap_err();
        assert!(matches!(err, aurora_core::Error::InvalidInput(_)));
        any_put.assert();
    }

    /// S3 验收 3：空负载 → 0 分片，直接提交 manifest（parts:0）。
    #[tokio::test]
    async fn dk08_chunk_empty_payload_completes_immediately() {
        let mut server = Server::new_async().await;
        let part_mocks = (0..1u32)
            .map(|i| {
                server
                    .mock("PUT", format!("/aurora/empty.part-{i:05}").as_str())
                    .expect(0)
                    .create()
            })
            .collect::<Vec<_>>();
        let manifest_mock = server
            .mock("PUT", "/aurora/empty.manifest.json")
            .match_body(Matcher::Regex("\"parts\":0".into()))
            .with_status(201)
            .expect(1)
            .create();
        let mut target = WebDavTarget::new();
        let conn = connect_to(&server, &mut target).await;

        let mut session = UploadSession::new("empty", 0, 64);
        assert_eq!(session.total_parts(), 0);
        let uploader = ChunkedUploader::new(&target, &conn);
        let progress = uploader.upload(&mut session, b"").await.unwrap();
        assert_eq!(
            progress,
            UploadProgress::Completed {
                manifest_path: "/aurora/empty.manifest.json".into()
            }
        );
        for m in &part_mocks {
            m.assert();
        }
        manifest_mock.assert();
    }

    /// S3 验收 4：读侧 manifest 缺失 → NotFound（fail-closed）。
    #[tokio::test]
    async fn dk08_chunk_download_missing_manifest_fails_closed() {
        let mut server = Server::new_async().await;
        let get_manifest = server
            .mock("GET", "/aurora/ghost.manifest.json")
            .with_status(404)
            .expect(1)
            .create();
        let mut target = WebDavTarget::new();
        let conn = connect_to(&server, &mut target).await;

        let reader = ChunkedUploader::new(&target, &conn);
        let err = reader.download_reassembled("ghost").await.unwrap_err();
        assert!(
            matches!(err, aurora_core::Error::NotFound(_)),
            "manifest 缺失必须 NotFound，得到 {err:?}"
        );
        get_manifest.assert();
    }
}
