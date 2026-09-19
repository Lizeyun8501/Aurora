//! WebDAV 增量同步适配器 (WebDAV Incremental Sync Adapter) — DK-08 第一切片。
//!
//! 首个实现 [`SyncTarget`] 增量三原语（`send_update` / `recv_update` /
//! `sync_version`，V26 DK-00 契约冻结）的**真网络**传输适配器：
//!
//! - `sync_version`：GET `{base}/aurora/index.json` → 该 doc 的远端版本
//!   （index 或条目缺失 → `Ok(None)`，首次同步语义）。
//! - `recv_update`：比对远端版本与本端已持有版本（版本水位），**仅远端
//!   较新时**才 GET oplog 本体；版本相同返回空 `Vec` 且不发数据请求
//!   —— 增量语义（非全量回退）由测试的请求次数断言证明。
//! - `send_update`：PUT oplog + 读-改-写 index.json（版本号单调递增）。
//!
//! # 路径布局（约定）
//!
//! ```text
//! {base}/aurora/index.json        — doc_id → {version, etag} 清单
//! {base}/aurora/{doc_id}.oplog    — Loro OpLog 快照字节
//! ```
//!
//! index 键 = 原始 doc_id（JSON 键无路径约束）；oplog 文件名 =
//! percent-encode(doc_id)（保证单段路径安全）。
//!
//! # 认证（受冻结接口约束的临时方案）
//!
//! `Endpoint`（aurora-core，DK-00 契约冻结）只有 `url` + `protocol` 两个字段，
//! **没有** username/password，`SyncProtocol` 也无 WebDAV 变体。本适配器按
//! 任务书要求不改 aurora-core，采用 URL userinfo 携带 Basic Auth 凭据：
//!
//! ```text
//! https://user:password@dav.example.com/dav
//! ```
//!
//! 显式凭据字段 / WebDav 协议变体的需求已提交
//! `issues/wf/bravo-request-webdav-endpoint-auth.md`，待 Alpha 冻结窗口合入。
//!
//! # 已知限制（第二切片解决）
//!
//! - index.json 的读-改-写非原子（两设备并发 `send_update` 后写者胜），
//!   真冲突合并属 DK-08 切片二（loro merge + `.conflict-{ts}.md` 副本）。
//! - `SyncConfig.max_retries` / `batch_size` 不在本适配器消费：重试由
//!   offline_queue / 路由降级承担，分批由上层按单文档原语循环实现。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use aurora_core::traits::sync_target::{
    Connection, DocSet, Endpoint, SyncEvent, SyncReport, SyncTarget, UpdatePayload,
};
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

/// 远端路径中的固定目录（任务书 §5.1 约定）。
const AURORA_DIR: &str = "aurora";
/// 远端清单文件名。
const INDEX_FILE: &str = "index.json";

/// index.json 单条目：`doc_id → {version, etag}`。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebDavIndexEntry {
    /// 远端文档版本（单调递增，增量判定唯一依据）。
    pub version: u64,
    /// oplog 本体 ETag（PUT 响应回填；版本判定不依赖它）。
    #[serde(default)]
    pub etag: Option<String>,
}

/// 远端清单：`{base}/aurora/index.json` 的反序列化形态（doc_id → 条目直映射）。
pub type WebDavIndex = HashMap<String, WebDavIndexEntry>;

/// [`WebDavTarget::push_with_merge`] 的推送结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushOutcome {
    /// 远端未分叉：本地内容直接快进推送（无合并发生）。
    FastForward { new_version: u64 },
    /// 远端已分叉：CRDT 自动合并后推送（需 `loro-crdt` feature）。
    AutoMerged {
        /// 检测到的远端实际版本。
        remote_base: u64,
        /// 合并结果写入后的新版本。
        merged_version: u64,
    },
}

/// 同步事件回调（`watch` 注册，Arc 包裹以便快照广播）。
type SyncEventCallback = Arc<dyn Fn(SyncEvent) + Send + Sync>;

/// WebDAV 同步目标 — [`SyncTarget`] 的真 HTTP 实现（reqwest + Basic Auth）。
///
/// WebDAV 服务器无连接状态，增量由**版本水位**驱动：本结构维护每文档
/// 「本端已持有的远端版本」，`recv_update` / `send_update` 成功后推进，
/// 作为增量判定基准（本端可更新或版本相同 → 拉取侧空操作）。
pub struct WebDavTarget {
    /// HTTP 客户端（`connect_with_config` 可按超时配置重建）。
    http: RwLock<reqwest::Client>,
    /// 事件回调（`watch` 注册，fire-and-forget）。
    watchers: Mutex<Vec<SyncEventCallback>>,
    /// doc_id → 本端已持有的远端版本（增量判定基准）。
    tracked: Mutex<HashMap<String, u64>>,
}

impl Default for WebDavTarget {
    fn default() -> Self {
        Self::new()
    }
}

impl WebDavTarget {
    /// 创建适配器（默认 30s 请求超时；`connect_with_config` 可覆盖）。
    pub fn new() -> Self {
        Self {
            http: RwLock::new(build_client(30_000)),
            watchers: Mutex::new(Vec::new()),
            tracked: Mutex::new(HashMap::new()),
        }
    }

    /// 测试/诊断：本端当前版本水位快照。
    #[cfg(test)]
    fn tracked_snapshot(&self) -> HashMap<String, u64> {
        self.tracked.lock().clone()
    }

    /// 广播事件（先快照回调列表再调用，避免持锁回调重入死锁）。
    fn emit(&self, event: SyncEvent) {
        for cb in self.watchers.lock().clone() {
            cb(event.clone());
        }
    }

    /// 数据操作的错误事件接线：失败时先广播再原样返回（fail-closed）。
    async fn emit_err(&self, e: &aurora_core::Error) {
        self.emit(SyncEvent::Error {
            message: e.to_string(),
        });
    }

    /// 发起 GET index.json，返回远端清单。
    ///
    /// 404 → `Ok(None)`（首次同步语义：远端尚无清单 = 全部文档无远端版本）。
    /// 401/403 → `Err(PermissionDenied)`（fail-closed，不静默当空数据处理）。
    async fn fetch_index(
        &self,
        conn: &Connection,
    ) -> Result<Option<WebDavIndex>, aurora_core::Error> {
        let (base, auth) = split_endpoint(conn)?;
        let url = format!("{base}/{AURORA_DIR}/{INDEX_FILE}");
        let client = self.http.read().clone();
        let mut req = client.get(&url);
        if let Some((u, p)) = auth {
            req = req.basic_auth(u, Some(p));
        }
        let resp = req.send().await.map_err(http_err)?;
        let status = resp.status();
        if status.is_success() {
            let bytes = resp.bytes().await.map_err(http_err)?;
            let index: WebDavIndex =
                serde_json::from_slice(&bytes).map_err(aurora_core::Error::Serialization)?;
            debug!(url, docs = index.len(), "webdav index fetched");
            Ok(Some(index))
        } else if status.as_u16() == 404 {
            Ok(None)
        } else if matches!(status.as_u16(), 401 | 403) {
            Err(aurora_core::Error::PermissionDenied(format!(
                "webdav auth failed ({url}): {status}"
            )))
        } else {
            Err(aurora_core::Error::Network(format!(
                "webdav index fetch failed ({url}): HTTP {status}"
            )))
        }
    }

    /// GET 任意文件 `{base}{path}`（path 需以 `/` 开头）。
    ///
    /// 404 → `Err(NotFound)`（fail-closed；调用方自行决定缺省语义，
    /// 与 `fetch_index` 的 `Ok(None)` 首次同步语义不同）。
    pub(crate) async fn get_file(
        &self,
        conn: &Connection,
        path: &str,
    ) -> Result<Vec<u8>, aurora_core::Error> {
        let (base, auth) = split_endpoint(conn)?;
        let url = format!("{base}{path}");
        let client = self.http.read().clone();
        let mut req = client.get(&url);
        if let Some((u, p)) = auth {
            req = req.basic_auth(u, Some(p));
        }
        let resp = req.send().await.map_err(http_err)?;
        let status = resp.status();
        if status.is_success() {
            Ok(resp.bytes().await.map_err(http_err)?.to_vec())
        } else if status.as_u16() == 404 {
            Err(aurora_core::Error::NotFound(format!(
                "webdav file missing: {path}"
            )))
        } else if matches!(status.as_u16(), 401 | 403) {
            Err(aurora_core::Error::PermissionDenied(format!(
                "webdav auth failed ({url}): {status}"
            )))
        } else {
            Err(aurora_core::Error::Network(format!(
                "webdav get failed ({url}): HTTP {status}"
            )))
        }
    }

    /// GET 单文档 oplog 本体（仅远端较新时由 `recv_update`/`sync` 调用）。
    async fn fetch_oplog(
        &self,
        conn: &Connection,
        doc_id: &str,
    ) -> Result<Vec<u8>, aurora_core::Error> {
        let (base, auth) = split_endpoint(conn)?;
        let url = format!("{base}/{AURORA_DIR}/{}.oplog", encode_doc_id(doc_id));
        let client = self.http.read().clone();
        let mut req = client.get(&url);
        if let Some((u, p)) = auth {
            req = req.basic_auth(u, Some(p));
        }
        let resp = req.send().await.map_err(http_err)?;
        let status = resp.status();
        if status.is_success() {
            Ok(resp.bytes().await.map_err(http_err)?.to_vec())
        } else if status.as_u16() == 404 {
            // index 声称有版本但本体缺失 → 远端不一致，大声失败
            Err(aurora_core::Error::Network(format!(
                "webdav oplog missing though listed in index ({url})"
            )))
        } else if matches!(status.as_u16(), 401 | 403) {
            Err(aurora_core::Error::PermissionDenied(format!(
                "webdav auth failed ({url}): {status}"
            )))
        } else {
            Err(aurora_core::Error::Network(format!(
                "webdav oplog fetch failed ({url}): HTTP {status}"
            )))
        }
    }

    /// PUT 任意文件到 `{base}{path}`（path 需以 `/` 开头），成功返回响应 ETag。
    pub(crate) async fn put_file(
        &self,
        conn: &Connection,
        path: &str,
        body: &[u8],
    ) -> Result<Option<String>, aurora_core::Error> {
        let (base, auth) = split_endpoint(conn)?;
        let url = format!("{base}{path}");
        let client = self.http.read().clone();
        let mut req = client.put(&url).body(body.to_vec());
        if let Some((u, p)) = auth {
            req = req.basic_auth(u, Some(p));
        }
        let resp = req.send().await.map_err(http_err)?;
        let status = resp.status();
        if status.is_success() {
            Ok(resp
                .headers()
                .get(reqwest::header::ETAG)
                .and_then(|v| v.to_str().ok())
                .map(str::to_string))
        } else if matches!(status.as_u16(), 401 | 403) {
            Err(aurora_core::Error::PermissionDenied(format!(
                "webdav auth failed ({url}): {status}"
            )))
        } else {
            Err(aurora_core::Error::Network(format!(
                "webdav put failed ({url}): HTTP {status}"
            )))
        }
    }

    /// PUT oplog 本体（`{base}/aurora/{doc_id}.oplog`），成功返回响应 ETag。
    async fn put_oplog(
        &self,
        conn: &Connection,
        doc_id: &str,
        ops: &[u8],
    ) -> Result<Option<String>, aurora_core::Error> {
        let path = format!("/{AURORA_DIR}/{}.oplog", encode_doc_id(doc_id));
        self.put_file(conn, &path, ops).await
    }

    /// 读-改-写 index.json：把 `entry` 合入调用方已取到的 `existing` 后整体 PUT。
    ///
    /// `existing` 由调用方传入（send 全程只读一次 index，避免双 GET）。
    async fn put_index_entry(
        &self,
        conn: &Connection,
        existing: Option<WebDavIndex>,
        doc_id: &str,
        entry: WebDavIndexEntry,
    ) -> Result<(), aurora_core::Error> {
        let mut index = existing.unwrap_or_default();
        index.insert(doc_id.to_string(), entry);
        let (base, auth) = split_endpoint(conn)?;
        let url = format!("{base}/{AURORA_DIR}/{INDEX_FILE}");
        let body = serde_json::to_vec(&index).map_err(aurora_core::Error::Serialization)?;
        let client = self.http.read().clone();
        let mut req = client.put(&url).body(body);
        if let Some((u, p)) = auth {
            req = req.basic_auth(u, Some(p));
        }
        let resp = req.send().await.map_err(http_err)?;
        let status = resp.status();
        if status.is_success() {
            Ok(())
        } else if matches!(status.as_u16(), 401 | 403) {
            Err(aurora_core::Error::PermissionDenied(format!(
                "webdav auth failed ({url}): {status}"
            )))
        } else {
            Err(aurora_core::Error::Network(format!(
                "webdav index put failed ({url}): HTTP {status}"
            )))
        }
    }

    /// 本端已持有的版本水位（无记录 = 0）。
    fn local_version(&self, doc_id: &str) -> u64 {
        self.tracked.lock().get(doc_id).copied().unwrap_or(0)
    }

    /// 统一的真冲突错误（fail-safe：不覆盖远端、不丢本地）。
    fn sync_conflict_err(doc_id: &str, base: u64, remote: u64) -> aurora_core::Error {
        aurora_core::Error::SyncConflict {
            note_id: doc_id.to_string(),
            local: format!("base v{base}"),
            remote: format!("v{remote}"),
        }
    }

    // ------------------------------------------------------------------
    // DK-08 S2：冲突感知推送 + 真冲突 .conflict-{ts}.md 副本
    // ------------------------------------------------------------------

    /// 冲突感知推送（S2 核心）。
    ///
    /// `local_oplog` 为本地文档快照字节，`base_version` 是它所基于的远端版本
    /// （= 本端水位）。判定逻辑：
    ///
    /// - 远端未分叉（v == base）→ 快进推送 → [`PushOutcome::FastForward`]；
    /// - 远端已分叉（v > base）+ `loro-crdt` feature → 拉 oplog 做
    ///   **CRDT 自动合并**（import 双方 → export snapshot）→ 推送合并结果
    ///   → [`PushOutcome::AutoMerged`]；
    /// - 远端已分叉且无合并能力（默认 feature 集）→ `Err(SyncConflict)`
    ///   **fail-safe**：零写入、零数据请求，两侧数据均未受损；
    ///   调用方随后用 [`WebDavTarget::upload_conflict_copy`] 保存本地快照，
    ///   并在 [`crate::conflict::ConflictArtifactStore`] 登记。
    pub async fn push_with_merge(
        &self,
        conn: &Connection,
        doc_id: &str,
        local_oplog: &[u8],
        base_version: u64,
    ) -> Result<PushOutcome, aurora_core::Error> {
        let outcome = self
            .push_with_merge_inner(conn, doc_id, local_oplog, base_version)
            .await;
        if let Err(e) = &outcome {
            self.emit_err(e).await;
        }
        outcome
    }

    async fn push_with_merge_inner(
        &self,
        conn: &Connection,
        doc_id: &str,
        local_oplog: &[u8],
        base_version: u64,
    ) -> Result<PushOutcome, aurora_core::Error> {
        let existing = self.fetch_index(conn).await?;
        let remote_ver = existing
            .as_ref()
            .and_then(|idx| idx.get(doc_id))
            .map(|e| e.version)
            .unwrap_or(0);

        // fail-fast：分叉且无合并能力 → 立刻冲突退出（零写入）
        if remote_ver > base_version && !Self::CAN_MERGE {
            debug!(
                doc_id,
                base = base_version,
                remote = remote_ver,
                "webdav push: diverged without merge capability"
            );
            return Err(Self::sync_conflict_err(doc_id, base_version, remote_ver));
        }

        let remote_oplog = if remote_ver > base_version {
            Some(self.fetch_oplog(conn, doc_id).await?)
        } else {
            None
        };

        let diverged = remote_oplog.is_some();
        let payload = match remote_oplog {
            None => local_oplog.to_vec(),
            Some(remote_bytes) => match Self::crdt_merge(local_oplog, &remote_bytes) {
                Ok(merged) => merged,
                // oplog 无法 import（非 Loro 内容等）→ 同样 fail-safe
                Err(_) => {
                    return Err(Self::sync_conflict_err(doc_id, base_version, remote_ver));
                }
            },
        };

        let new_version = remote_ver.max(base_version) + 1;
        let etag = self.put_oplog(conn, doc_id, &payload).await?;
        self.put_index_entry(
            conn,
            existing,
            doc_id,
            WebDavIndexEntry {
                version: new_version,
                etag,
            },
        )
        .await?;
        self.tracked.lock().insert(doc_id.to_string(), new_version);
        info!(
            doc_id,
            new_version,
            bytes = payload.len(),
            "webdav push_with_merge done"
        );
        self.emit(SyncEvent::Progress { progress: 1.0 });
        Ok(if diverged {
            PushOutcome::AutoMerged {
                remote_base: remote_ver,
                merged_version: new_version,
            }
        } else {
            PushOutcome::FastForward { new_version }
        })
    }

    /// 真冲突副本：把人类可读快照上传为
    /// `{base}/aurora/{doc_id}.conflict-{ts}.md`，返回远端路径。
    ///
    /// 不触碰主线 oplog 与 index —— 副本仅为人工调和存证。
    pub async fn upload_conflict_copy(
        &self,
        conn: &Connection,
        doc_id: &str,
        artifact: &[u8],
    ) -> Result<String, aurora_core::Error> {
        let outcome = self
            .upload_conflict_copy_inner(conn, doc_id, artifact)
            .await;
        if let Err(e) = &outcome {
            self.emit_err(e).await;
        }
        outcome
    }

    async fn upload_conflict_copy_inner(
        &self,
        conn: &Connection,
        doc_id: &str,
        artifact: &[u8],
    ) -> Result<String, aurora_core::Error> {
        let ts = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
        let path = format!("/{AURORA_DIR}/{}.conflict-{ts}.md", encode_doc_id(doc_id));
        self.put_file(conn, &path, artifact).await?;
        info!(doc_id, path = %path, bytes = artifact.len(), "conflict copy uploaded");
        Ok(path)
    }

    /// 合并能力开关（随 feature 集编译期确定）。
    const CAN_MERGE: bool = cfg!(feature = "loro-crdt");

    /// CRDT 自动合并：双方 oplog（Loro snapshot/updates）import 进同一文档，
    /// 导出合并后的 snapshot。CRDT 语义保证双侧修改均保留、无数据丢失。
    #[cfg(feature = "loro-crdt")]
    fn crdt_merge(local: &[u8], remote: &[u8]) -> Result<Vec<u8>, aurora_core::Error> {
        use loro::ExportMode;
        let doc = loro::LoroDoc::new();
        doc.import(local)
            .map_err(|e| aurora_core::Error::Loro(format!("webdav merge: local import: {e}")))?;
        doc.import(remote)
            .map_err(|e| aurora_core::Error::Loro(format!("webdav merge: remote import: {e}")))?;
        doc.export(ExportMode::snapshot())
            .map_err(|e| aurora_core::Error::Loro(format!("webdav merge: snapshot export: {e}")))
    }

    /// 默认 feature 集无 loro：不做合并（`CAN_MERGE = false` 已在上游拦截）。
    #[cfg(not(feature = "loro-crdt"))]
    fn crdt_merge(_local: &[u8], _remote: &[u8]) -> Result<Vec<u8>, aurora_core::Error> {
        Err(Self::sync_conflict_err("", 0, 0))
    }

    async fn send_update_inner(
        &self,
        conn: &Connection,
        update: &UpdatePayload,
    ) -> Result<(), aurora_core::Error> {
        // index 全程只读一次，读-改-写复用同一份（避免双 GET）
        let existing = self.fetch_index(conn).await?;
        let remote_ver = existing
            .as_ref()
            .and_then(|idx| idx.get(&update.doc_id))
            .map(|e| e.version)
            .unwrap_or(0);
        let new_version = remote_ver.max(self.local_version(&update.doc_id)) + 1;
        let etag = self.put_oplog(conn, &update.doc_id, &update.ops).await?;
        self.put_index_entry(
            conn,
            existing,
            &update.doc_id,
            WebDavIndexEntry {
                version: new_version,
                etag,
            },
        )
        .await?;
        self.tracked
            .lock()
            .insert(update.doc_id.clone(), new_version);
        info!(
            doc_id = %update.doc_id,
            version = new_version,
            bytes = update.ops.len(),
            "webdav pushed"
        );
        self.emit(SyncEvent::Progress { progress: 1.0 });
        Ok(())
    }

    async fn sync_version_inner(
        &self,
        conn: &Connection,
        doc_id: &str,
    ) -> Result<Option<u64>, aurora_core::Error> {
        let remote = self
            .fetch_index(conn)
            .await?
            .and_then(|idx| idx.get(doc_id).map(|e| e.version));
        Ok(remote)
    }

    async fn recv_update_inner(
        &self,
        conn: &Connection,
        doc_id: &str,
    ) -> Result<Vec<u8>, aurora_core::Error> {
        let remote = self
            .fetch_index(conn)
            .await?
            .and_then(|idx| idx.get(doc_id).map(|e| e.version));
        let Some(remote_ver) = remote else {
            debug!(doc_id, "webdav recv: no remote version, nothing to receive");
            return Ok(Vec::new());
        };
        let local = self.local_version(doc_id);
        if remote_ver <= local {
            // 增量语义核心：版本相同（或本端更新）→ 不取 oplog 本体
            debug!(
                doc_id,
                remote = remote_ver,
                local,
                "webdav recv: up-to-date, skip body"
            );
            return Ok(Vec::new());
        }
        let bytes = self.fetch_oplog(conn, doc_id).await?;
        self.tracked.lock().insert(doc_id.to_string(), remote_ver);
        debug!(
            doc_id,
            version = remote_ver,
            bytes = bytes.len(),
            "webdav received"
        );
        Ok(bytes)
    }
}

/// 按超时毫秒构建 HTTP 客户端。
fn build_client(timeout_ms: u64) -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(timeout_ms.max(1)))
        .build()
        .unwrap_or_default()
}

/// 把传输层错误统一映射为 `aurora_core::Error::Network`。
fn http_err(e: reqwest::Error) -> aurora_core::Error {
    aurora_core::Error::Network(format!("webdav transport error: {e}"))
}

/// 从 `Connection.endpoint.url` 拆出 (base_url, Basic Auth 凭据)。
///
/// base = 去尾斜杠的 URL；userinfo（`user:pass@`）从 authority 段剥离，
/// 不参与路径拼接。非 http(s) 或缺 authority → `InvalidInput`。
fn split_endpoint(
    conn: &Connection,
) -> Result<(String, Option<(String, String)>), aurora_core::Error> {
    split_url(&conn.endpoint.url)
}

/// [`split_endpoint`] 的纯函数实现（便于单测）。
fn split_url(raw: &str) -> Result<(String, Option<(String, String)>), aurora_core::Error> {
    let (scheme, rest) = raw
        .split_once("://")
        .ok_or_else(|| invalid_url(raw, "missing scheme"))?;
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return Err(invalid_url(raw, "scheme must be http(s) for WebDAV"));
    }
    // authority = 第一个 '/' 之前的部分
    let authority_end = rest.find('/').unwrap_or(rest.len());
    let (authority, path) = rest.split_at(authority_end);
    if authority.is_empty() {
        return Err(invalid_url(raw, "missing host"));
    }
    // userinfo = authority 最后一个 '@' 之前的部分（密码可含 '@'）；
    // host 段必须剥离凭据 —— 凭据只进 Authorization 头，不进 URL
    let (host, auth) = match authority.rsplit_once('@') {
        Some((userinfo, host)) => {
            let (u, p) = userinfo.split_once(':').ok_or_else(|| {
                aurora_core::Error::InvalidInput(format!(
                    "webdav endpoint userinfo must be `user:password`: {raw}"
                ))
            })?;
            (host, Some((u.to_string(), p.to_string())))
        }
        None => (authority, None),
    };
    if host.is_empty() {
        return Err(invalid_url(raw, "missing host"));
    }
    let base = format!("{scheme}://{host}{path}");
    let base = base.trim_end_matches('/').to_string();
    Ok((base, auth))
}

fn invalid_url(raw: &str, why: &str) -> aurora_core::Error {
    aurora_core::Error::InvalidInput(format!("invalid webdav endpoint `{raw}`: {why}"))
}

/// percent-encode doc_id 为单段安全路径（保留 RFC 3986 unreserved 字符）。
pub(crate) fn encode_doc_id(doc_id: &str) -> String {
    let mut out = String::with_capacity(doc_id.len());
    for &b in doc_id.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char)
            }
            _ => {
                out.push('%');
                out.push_str(&format!("{b:02X}"));
            }
        }
    }
    out
}

#[async_trait]
impl SyncTarget for WebDavTarget {
    async fn connect(&mut self, endpoint: &Endpoint) -> Result<Connection, aurora_core::Error> {
        // URL 合法性先行验证（fail-fast，而非拖到首个数据操作）
        split_url(&endpoint.url)?;
        let conn = Connection {
            id: format!("webdav-{}", uuid::Uuid::new_v4()),
            endpoint: endpoint.clone(),
        };
        info!(url = %endpoint.url, conn_id = %conn.id, "webdav target connected");
        self.emit(SyncEvent::Connected {
            conn_id: conn.id.clone(),
        });
        Ok(conn)
    }

    /// 覆写默认实现：按 `SyncConfig.timeout_ms` 重建 HTTP 客户端
    /// （超时语义在传输层落实，V20 §28.1；重试/分批语义见模块文档）。
    async fn connect_with_config(
        &mut self,
        endpoint: &Endpoint,
        config: &aurora_core::traits::sync_target::SyncConfig,
    ) -> Result<Connection, aurora_core::Error> {
        *self.http.write() = build_client(config.timeout_ms);
        self.connect(endpoint).await
    }

    /// 拉取侧全量遍历：对 `doc_set` 逐文档做增量判定（远端较新才取本体）。
    ///
    /// 推送方向不经 `sync`（本地 oplog 导出属上层钩子职责），
    /// 走 [`SyncTarget::send_update`] 原语。
    async fn sync(
        &self,
        conn: &Connection,
        doc_set: &DocSet,
    ) -> Result<SyncReport, aurora_core::Error> {
        let started = Instant::now();
        let index = match self.fetch_index(conn).await {
            Ok(index) => index,
            Err(e) => {
                self.emit_err(&e).await;
                return Err(e);
            }
        };
        let mut received = 0usize;
        let total = doc_set.doc_ids.len();
        for (i, doc_id) in doc_set.doc_ids.iter().enumerate() {
            let remote = index
                .as_ref()
                .and_then(|idx| idx.get(doc_id))
                .map(|e| e.version);
            if let Some(v) = remote {
                if v > self.local_version(doc_id) {
                    let bytes = match self.fetch_oplog(conn, doc_id).await {
                        Ok(bytes) => bytes,
                        Err(e) => {
                            self.emit_err(&e).await;
                            return Err(e);
                        }
                    };
                    self.tracked.lock().insert(doc_id.clone(), v);
                    received += 1;
                    debug!(doc_id, version = v, bytes = bytes.len(), "webdav pulled");
                }
            }
            self.emit(SyncEvent::Progress {
                progress: (i + 1) as f32 / total.max(1) as f32,
            });
        }
        Ok(SyncReport {
            sent_ops: 0,
            received_ops: received,
            duration_ms: started.elapsed().as_millis() as u64,
        })
    }

    /// ★ 增量发送：PUT oplog 本体 + 读-改-写 index.json。
    ///
    /// 版本号 = max(远端版本, 本端水位) + 1（单调递增，不回退）。
    async fn send_update(
        &self,
        conn: &Connection,
        update: &UpdatePayload,
    ) -> Result<(), aurora_core::Error> {
        let outcome = self.send_update_inner(conn, update).await;
        if let Err(e) = &outcome {
            self.emit_err(e).await;
        }
        outcome
    }

    /// ★ 增量接收：远端版本 <= 本端水位 → 空 `Vec`（**不发** oplog 数据请求）；
    /// 远端较新 → GET oplog 本体并推进水位。
    async fn recv_update(
        &self,
        conn: &Connection,
        doc_id: &str,
    ) -> Result<Vec<u8>, aurora_core::Error> {
        let outcome = self.recv_update_inner(conn, doc_id).await;
        if let Err(e) = &outcome {
            self.emit_err(e).await;
        }
        outcome
    }

    /// ★ 远端文档版本查询：index.json 中该 doc 的 version；
    /// index 或条目缺失 → `Ok(None)`（首次同步语义，显式声明）。
    async fn sync_version(
        &self,
        conn: &Connection,
        doc_id: &str,
    ) -> Result<Option<u64>, aurora_core::Error> {
        let outcome = self.sync_version_inner(conn, doc_id).await;
        if let Err(e) = &outcome {
            self.emit_err(e).await;
        }
        outcome
    }

    fn watch(&self, callback: Box<dyn Fn(SyncEvent) + Send + Sync>) {
        self.watchers.lock().push(Arc::from(callback));
    }

    async fn disconnect(&self, conn: &Connection) -> Result<(), aurora_core::Error> {
        info!(conn_id = %conn.id, "webdav target disconnected");
        self.emit(SyncEvent::Disconnected {
            conn_id: conn.id.clone(),
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aurora_core::traits::sync_target::SyncProtocol;
    use mockito::{Matcher, Server};

    /// 测试端点：mockito 服务器。
    fn endpoint_for(server: &Server) -> Endpoint {
        Endpoint {
            url: server.url(),
            // SyncProtocol 无 WebDAV 变体（见 request 文档）；适配器不匹配该字段
            protocol: SyncProtocol::WebSocket,
        }
    }

    /// 带 userinfo 凭据的端点（`user:pass@host`）。
    fn endpoint_with_auth(server: &Server, user: &str, pass: &str) -> Endpoint {
        Endpoint {
            url: server
                .url()
                .replacen("://", &format!("://{user}:{pass}@"), 1),
            protocol: SyncProtocol::WebSocket,
        }
    }

    async fn connect_to(server: &Server, target: &mut WebDavTarget) -> Connection {
        let ep = endpoint_for(server);
        target.connect(&ep).await.unwrap()
    }

    /// 建立 index mock（200 + JSON 清单，expect = 精确命中次数）。
    async fn mock_index(server: &mut Server, body: &'static str, hits: usize) -> mockito::Mock {
        server
            .mock("GET", "/aurora/index.json")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body)
            .expect(hits)
            .create_async()
            .await
    }

    /// 建立 oplog mock（200 + 字节体，expect = 精确命中次数）。
    async fn mock_oplog(
        server: &mut Server,
        doc_id: &str,
        body: &'static str,
        hits: usize,
    ) -> mockito::Mock {
        let path = format!("/aurora/{}.oplog", encode_doc_id(doc_id));
        server
            .mock("GET", path.as_str())
            .with_status(200)
            .with_body(body)
            .expect(hits)
            .create_async()
            .await
    }

    /// DK-08 验收 1：增量语义 —— 远端 v2 / 本地 v1 → 拉到 bytes；
    /// 本地 v2（版本相同）→ 空 Vec，且**不 GET oplog 本体**（请求次数断言，
    /// 「非全量回退」的证据）。
    #[tokio::test]
    async fn dk08_webdav_recv_incremental_only() {
        // ── 阶段 A：本地从 0 收到 v1（建立「本地 v1」水位）──
        let mut server_a = Server::new_async().await;
        let index_a = mock_index(&mut server_a, r#"{"note-1":{"version":1}}"#, 1).await;
        let oplog_a = mock_oplog(&mut server_a, "note-1", "OPS-V1", 1).await;
        let mut target = WebDavTarget::new();
        let conn_a = connect_to(&server_a, &mut target).await;
        let got = target.recv_update(&conn_a, "note-1").await.unwrap();
        assert_eq!(got, b"OPS-V1");
        assert_eq!(target.tracked_snapshot()["note-1"], 1);
        index_a.assert();
        oplog_a.assert();

        // ── 阶段 B：远端 v2、本地 v1 → 拉到 v2 bytes ──
        let mut server_b = Server::new_async().await;
        let index_b = mock_index(&mut server_b, r#"{"note-1":{"version":2}}"#, 1).await;
        let oplog_b = mock_oplog(&mut server_b, "note-1", "OPS-V2", 1).await;
        let conn_b = Connection {
            id: "conn-b".into(),
            endpoint: endpoint_for(&server_b),
        };
        let got = target.recv_update(&conn_b, "note-1").await.unwrap();
        assert_eq!(got, b"OPS-V2");
        assert_eq!(target.tracked_snapshot()["note-1"], 2);
        index_b.assert();
        oplog_b.assert();

        // ── 阶段 C：远端 v2、本地 v2 → 空 Vec 且零次 oplog GET ──
        let mut server_c = Server::new_async().await;
        let index_c = mock_index(&mut server_c, r#"{"note-1":{"version":2}}"#, 1).await;
        let oplog_c = mock_oplog(&mut server_c, "note-1", "OPS-V2", 0).await;
        let conn_c = Connection {
            id: "conn-c".into(),
            endpoint: endpoint_for(&server_c),
        };
        let got = target.recv_update(&conn_c, "note-1").await.unwrap();
        assert!(got.is_empty(), "版本相同必须返回空 Vec（无更新）");
        index_c.assert();
        oplog_c.assert(); // expect(0)：版本相同路径不得 GET oplog 本体（增量证据）
    }

    /// DK-08 验收 2：send PUT 后 sync_version 返回新版本，recv 拉回 bytes
    /// 与发送一致（roundtrip）。
    #[tokio::test]
    async fn dk08_webdav_send_then_recv_roundtrip() {
        // ── 发送侧：首次 send（index 404 → 版本从 1 起）──
        let mut server_s = Server::new_async().await;
        let get_index_404 = server_s
            .mock("GET", "/aurora/index.json")
            .with_status(404)
            .expect(1)
            .create_async()
            .await;
        let put_oplog = server_s
            .mock("PUT", "/aurora/note-1.oplog")
            .match_body(Matcher::Exact("ROUNDTRIP-OPS".into()))
            .with_status(201)
            .with_header("ETag", "\"etag-1\"")
            .expect(1)
            .create_async()
            .await;
        let put_index = server_s
            .mock("PUT", "/aurora/index.json")
            // AllOf 合并两个断言（match_body 为覆盖语义，不能连调两次）
            .match_body(Matcher::AllOf(vec![
                Matcher::Regex("\"version\":1".to_string()),
                Matcher::Regex("note-1".to_string()),
            ]))
            .with_status(204)
            .expect(1)
            .create_async()
            .await;
        let mut target = WebDavTarget::new();
        let conn = connect_to(&server_s, &mut target).await;
        target
            .send_update(
                &conn,
                &UpdatePayload {
                    doc_id: "note-1".into(),
                    ops: b"ROUNDTRIP-OPS".to_vec(),
                },
            )
            .await
            .unwrap();
        put_oplog.assert(); // oplog 本体必须已 PUT（含字节级 body 断言）
        put_index.assert(); // index 必须已更新（含 version:1 断言）
        get_index_404.assert(); // send 前读 index 确认远端版本
        assert_eq!(target.tracked_snapshot()["note-1"], 1);

        // ── 接收侧（模拟另一台设备）：sync_version 看到新版本 → recv 拉回 ──
        let mut server_r = Server::new_async().await;
        let index_after = mock_index(
            &mut server_r,
            r#"{"note-1":{"version":1,"etag":"\"etag-1\""}}"#,
            2,
        )
        .await;
        let oplog_after = mock_oplog(&mut server_r, "note-1", "ROUNDTRIP-OPS", 1).await;
        let mut receiver = WebDavTarget::new();
        let conn_r = connect_to(&server_r, &mut receiver).await;
        assert_eq!(
            receiver.sync_version(&conn_r, "note-1").await.unwrap(),
            Some(1),
            "send 后远端版本应为 1"
        );
        let got = receiver.recv_update(&conn_r, "note-1").await.unwrap();
        assert_eq!(got, b"ROUNDTRIP-OPS", "recv 拉回的 bytes 必须与发送一致");
        index_after.assert(); // sync_version + recv 各一次
        oplog_after.assert();
    }

    /// DK-08 验收 3：401 → Err（fail-closed，不静默返回空数据）。
    ///
    /// mock 只匹配带 `Authorization` 头的请求 —— 若适配器不发凭据，
    /// mockito 将以 404 应答（index 404 → Ok(None)），本测试随即失败，
    /// 隐式证明 Basic Auth 头确实随请求发送。
    #[tokio::test]
    async fn dk08_webdav_auth_failure_fails_closed() {
        let mut server = Server::new_async().await;
        let index_401 = server
            .mock("GET", "/aurora/index.json")
            .match_header("authorization", Matcher::Any)
            .with_status(401)
            .expect(2)
            .create_async()
            .await;
        let mut target = WebDavTarget::new();
        let ep = endpoint_with_auth(&server, "alice", "secret");
        let conn = target.connect(&ep).await.unwrap();

        let v = target.sync_version(&conn, "note-1").await;
        assert!(
            matches!(v, Err(aurora_core::Error::PermissionDenied(_))),
            "401 必须 Err(PermissionDenied)，得到 {v:?}"
        );
        let got = target.recv_update(&conn, "note-1").await;
        assert!(
            matches!(got, Err(aurora_core::Error::PermissionDenied(_))),
            "401 不得静默返回空数据，得到 {got:?}"
        );
        index_401.assert();
    }

    /// DK-08 验收 4：index.json 404 → 全部 doc 版本 None（首次同步语义），
    /// 且 recv 返回空 Vec、零次 oplog 请求。
    #[tokio::test]
    async fn dk08_webdav_index_missing_treated_as_empty() {
        let mut server = Server::new_async().await;
        let index_404 = server
            .mock("GET", "/aurora/index.json")
            .with_status(404)
            .expect(2)
            .create_async()
            .await;
        let oplog_unused = mock_oplog(&mut server, "note-1", "NEVER", 0).await;
        let mut target = WebDavTarget::new();
        let conn = connect_to(&server, &mut target).await;

        assert_eq!(
            target.sync_version(&conn, "note-1").await.unwrap(),
            None,
            "index 缺失 → 版本 None（首次同步语义）"
        );
        let got = target.recv_update(&conn, "note-1").await.unwrap();
        assert!(got.is_empty(), "index 缺失 → 无远端数据可收");
        oplog_unused.assert(); // expect(0)：index 缺失时不得请求 oplog 本体
        index_404.assert(); // sync_version + recv 各一次
    }

    /// sync 拉取遍历：多文档增量 + 报告计数（received_ops 只数真拉取的）。
    #[tokio::test]
    async fn sync_pulls_only_stale_docs() {
        let mut server = Server::new_async().await;
        let index = mock_index(
            &mut server,
            r#"{"a":{"version":2},"b":{"version":1},"c":{"version":5}}"#,
            1,
        )
        .await;
        let oplog_a = mock_oplog(&mut server, "a", "A2", 0).await;
        let oplog_b = mock_oplog(&mut server, "b", "B1", 1).await;
        let oplog_c = mock_oplog(&mut server, "c", "C5", 1).await;
        let mut target = WebDavTarget::new();
        let conn = connect_to(&server, &mut target).await;

        // 水位：a 已有 v2（不拉）、b/c 无记录（拉）
        target.tracked.lock().insert("a".into(), 2);
        let doc_set = DocSet {
            doc_ids: vec!["a".into(), "b".into(), "c".into()],
        };
        let report = target.sync(&conn, &doc_set).await.unwrap();
        assert_eq!(report.received_ops, 2, "只统计真拉取的文档");
        assert_eq!(report.sent_ops, 0);
        oplog_a.assert(); // expect(0)：水位已同步的文档不取本体
        oplog_b.assert();
        oplog_c.assert();
        index.assert(); // index 只取一次（非逐文档 N 次）
        assert_eq!(target.tracked_snapshot()["c"], 5);
    }

    /// userinfo 拆分：凭据剥离出 base URL（密码可含 `@`）。
    #[test]
    fn split_url_extracts_userinfo() {
        let (base, auth) = split_url("https://alice:p@ss@dav.example.com/dav/").unwrap();
        assert_eq!(base, "https://dav.example.com/dav");
        assert_eq!(auth, Some(("alice".into(), "p@ss".into())));
        // 无凭据端点
        let (base, auth) = split_url("https://dav.example.com").unwrap();
        assert_eq!(base, "https://dav.example.com");
        assert_eq!(auth, None);
    }

    /// 非 http(s) scheme 与缺 host 的 fail-fast 拒绝。
    #[test]
    fn split_url_rejects_non_http() {
        assert!(split_url("ftp://host/dav").is_err());
        assert!(split_url("webdav://host").is_err());
        assert!(split_url("https://").is_err());
        assert!(split_url("host/dav").is_err());
    }

    /// doc_id 路径编码：unreserved 直通，其余 percent-encode（`:` 等）。
    #[test]
    fn encode_doc_id_is_path_safe() {
        assert_eq!(encode_doc_id("note-1_a.B~"), "note-1_a.B~");
        assert_eq!(encode_doc_id("note:42"), "note%3A42");
        assert_eq!(encode_doc_id("a/b"), "a%2Fb");
        assert_eq!(encode_doc_id("空"), "%E7%A9%BA");
    }

    /// watch 回调真实触发（Connected 生命周期事件 + Error 事件）。
    #[tokio::test]
    async fn watch_receives_lifecycle_events() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let mut server = Server::new_async().await;
        let index_401 = server
            .mock("GET", "/aurora/index.json")
            .with_status(401)
            .expect(1)
            .create_async()
            .await;
        let mut target = WebDavTarget::new();
        let connected = Arc::new(AtomicUsize::new(0));
        let errors = Arc::new(AtomicUsize::new(0));
        let cb_connected = connected.clone();
        let cb_errors = errors.clone();
        target.watch(Box::new(move |e| match e {
            SyncEvent::Connected { .. } => {
                cb_connected.fetch_add(1, Ordering::SeqCst);
            }
            SyncEvent::Error { .. } => {
                cb_errors.fetch_add(1, Ordering::SeqCst);
            }
            _ => {}
        }));
        let conn = connect_to(&server, &mut target).await;
        // 401 失败 → Error 事件广播 + Err 返回（fail-closed）
        assert!(target.sync_version(&conn, "note-1").await.is_err());
        assert_eq!(connected.load(Ordering::SeqCst), 1);
        assert_eq!(errors.load(Ordering::SeqCst), 1);
        index_401.assert();
    }

    // ==================================================================
    // DK-08 S2：冲突感知推送 + .conflict-{ts}.md 副本
    // ==================================================================

    /// S2 验收 1：远端未分叉（v == base）→ 快进推送，无合并、无多余读。
    #[tokio::test]
    async fn dk08_conflict_push_fast_forward() {
        let mut server = Server::new_async().await;
        let index = mock_index(&mut server, r#"{"note-1":{"version":1}}"#, 1).await;
        let oplog_unused = mock_oplog(&mut server, "note-1", "NEVER", 0).await;
        let put_oplog = server
            .mock("PUT", "/aurora/note-1.oplog")
            .match_body(Matcher::Exact("LOCAL-SNAPSHOT".into()))
            .with_status(201)
            .expect(1)
            .create_async()
            .await;
        let put_index = server
            .mock("PUT", "/aurora/index.json")
            .match_body(Matcher::Regex("\"version\":2".into()))
            .with_status(204)
            .expect(1)
            .create_async()
            .await;
        let mut target = WebDavTarget::new();
        let conn = connect_to(&server, &mut target).await;

        let out = target
            .push_with_merge(&conn, "note-1", b"LOCAL-SNAPSHOT", 1)
            .await
            .unwrap();
        assert_eq!(out, PushOutcome::FastForward { new_version: 2 });
        assert_eq!(target.tracked_snapshot()["note-1"], 2);
        index.assert();
        oplog_unused.assert(); // 未分叉 → 不拉远端 oplog
        put_oplog.assert();
        put_index.assert();
    }

    /// S2 验收 2（默认 feature 集，无 loro）：远端分叉 → `Err(SyncConflict)`
    /// fail-safe —— **零写、零数据读**，两侧数据均未受损。
    #[tokio::test]
    #[cfg(not(feature = "loro-crdt"))]
    async fn dk08_conflict_diverged_fail_safe_zero_writes() {
        let mut server = Server::new_async().await;
        let index = mock_index(&mut server, r#"{"note-1":{"version":2}}"#, 1).await;
        let oplog_unused = mock_oplog(&mut server, "note-1", "REMOTE-OPS", 0).await;
        let put_oplog = server
            .mock("PUT", "/aurora/note-1.oplog")
            .expect(0)
            .create_async()
            .await;
        let put_index = server
            .mock("PUT", "/aurora/index.json")
            .expect(0)
            .create_async()
            .await;
        let mut target = WebDavTarget::new();
        let conn = connect_to(&server, &mut target).await;

        let err = target
            .push_with_merge(&conn, "note-1", b"LOCAL-SNAPSHOT", 1)
            .await
            .unwrap_err();
        assert!(
            matches!(err, aurora_core::Error::SyncConflict { .. }),
            "分叉必须报 SyncConflict，得到 {err:?}"
        );
        assert_eq!(
            target
                .tracked_snapshot()
                .get("note-1")
                .copied()
                .unwrap_or(0),
            0,
            "失败不推进水位"
        );
        index.assert();
        oplog_unused.assert();
        put_oplog.assert(); // 零写入：远端主线未被覆盖
        put_index.assert();
    }

    /// S2 验收 3：真冲突副本上传 `.conflict-{ts}.md`（路径格式断言）
    /// 并在 ConflictArtifactStore 登记。
    #[tokio::test]
    async fn dk08_conflict_upload_copy_and_register() {
        use crate::conflict::{ConflictArtifact, ConflictArtifactStore};

        let mut server = Server::new_async().await;
        let put_copy = server
            .mock(
                "PUT",
                Matcher::Regex(r"note-1\.conflict-\d{8}T\d{6}Z\.md".into()),
            )
            .match_body(Matcher::Exact("LOCAL-LOSING-SNAPSHOT".into()))
            .with_status(201)
            .expect(1)
            .create_async()
            .await;
        let mut target = WebDavTarget::new();
        let conn = connect_to(&server, &mut target).await;

        let path = target
            .upload_conflict_copy(&conn, "note-1", b"LOCAL-LOSING-SNAPSHOT")
            .await
            .unwrap();
        assert!(conflict_copy_path_ok(&path), "副本路径格式不符: {path}");
        put_copy.assert();

        let store = ConflictArtifactStore::new();
        let id = store.record(ConflictArtifact::new("note-1", &path, 1, 2));
        assert_eq!(store.pending_for_doc("note-1").len(), 1);
        store.mark_resolved(&id).unwrap();
        assert!(store.pending_for_doc("note-1").is_empty());
    }

    /// 校验 `.conflict-{ts}.md` 路径格式：`/aurora/{doc}.conflict-YYYYMMDDTHHMMSSZ.md`。
    fn conflict_copy_path_ok(path: &str) -> bool {
        const TS_LEN: usize = 16; // 8 日期 + 'T' + 6 时间 + 'Z'
        let Some(rest) = path.strip_prefix("/aurora/note-1.conflict-") else {
            return false;
        };
        let Some(ts) = rest.strip_suffix(".md") else {
            return false;
        };
        ts.len() == TS_LEN
            && ts.as_bytes()[8] == b'T'
            && ts.ends_with('Z')
            && ts
                .bytes()
                .enumerate()
                .all(|(i, b)| i == 8 || i == TS_LEN - 1 || b.is_ascii_digit())
    }

    // ---------------- loro-crdt：CRDT 自动合并 ----------------

    /// 构造 Loro 文档快照：可选基于 base 导入 + 可选插入编辑。
    #[cfg(feature = "loro-crdt")]
    fn doc_from(base: Option<&[u8]>, edit_at: Option<(usize, &str)>) -> Vec<u8> {
        use loro::{ExportMode, LoroDoc};
        let doc = LoroDoc::new();
        if let Some(b) = base {
            doc.import(b).expect("import base snapshot");
        }
        if let Some((pos, s)) = edit_at {
            doc.get_text("content").insert(pos, s).expect("insert");
        }
        doc.export(ExportMode::snapshot()).expect("export snapshot")
    }

    /// S2 验收 4a：crdt_merge 纯函数正确性 —— 双侧编辑都保留（CRDT 收敛）。
    #[test]
    #[cfg(feature = "loro-crdt")]
    fn crdt_merge_keeps_both_sides() {
        use loro::LoroDoc;
        let base = doc_from(None, Some((0, "Hello")));
        let local = doc_from(Some(&base), Some((5, " local")));
        let remote = doc_from(Some(&base), Some((0, "Hi ")));

        let merged = WebDavTarget::crdt_merge(&local, &remote).expect("merge");
        let final_doc = LoroDoc::new();
        final_doc.import(&merged).expect("import merged");
        let text = final_doc.get_text("content").to_string();
        assert!(
            text.contains("Hello") && text.contains("local") && text.contains("Hi"),
            "合并结果必须双侧保留，得到: {text}"
        );
    }

    /// S2 验收 4b：分叉 + loro → AutoMerged 全链路（HTTP 流 + 水位推进）。
    #[tokio::test]
    #[cfg(feature = "loro-crdt")]
    async fn dk08_conflict_automerge_push() {
        let base = doc_from(None, Some((0, "Hello")));
        let local = doc_from(Some(&base), Some((5, " local")));
        let remote = doc_from(Some(&base), Some((0, "Hi ")));

        let mut server = Server::new_async().await;
        let index = mock_index(&mut server, r#"{"note-1":{"version":2}}"#, 1).await;
        let oplog = server
            .mock("GET", "/aurora/note-1.oplog")
            .with_status(200)
            .with_body(remote.clone())
            .expect(1)
            .create_async()
            .await;
        let put_oplog = server
            .mock("PUT", "/aurora/note-1.oplog")
            .with_status(201)
            .expect(1)
            .create_async()
            .await;
        let put_index = server
            .mock("PUT", "/aurora/index.json")
            .match_body(Matcher::Regex("\"version\":3".into()))
            .with_status(204)
            .expect(1)
            .create_async()
            .await;
        let mut target = WebDavTarget::new();
        let conn = connect_to(&server, &mut target).await;

        let out = target
            .push_with_merge(&conn, "note-1", &local, 1)
            .await
            .unwrap();
        assert_eq!(
            out,
            PushOutcome::AutoMerged {
                remote_base: 2,
                merged_version: 3
            }
        );
        assert_eq!(target.tracked_snapshot()["note-1"], 3);
        index.assert(); // 只读一次 index
        oplog.assert(); // 分叉 → 拉远端 oplog
        put_oplog.assert(); // 推送合并结果
        put_index.assert();

        // 远端最终状态 = 合并快照，双侧内容保留
        // （mockito 是 stub，不能回放 PUT 内容；合并正确性已由 4a 覆盖）
    }

    /// S2 验收 4c（loro 特性下）：oplog 无法 import（非 Loro 内容）→
    /// 同样 fail-safe 冲突退出，零写入。
    #[tokio::test]
    #[cfg(feature = "loro-crdt")]
    async fn dk08_conflict_automerge_bad_payload_falls_back_to_conflict() {
        let mut server = Server::new_async().await;
        let index = mock_index(&mut server, r#"{"note-1":{"version":2}}"#, 1).await;
        let oplog = server
            .mock("GET", "/aurora/note-1.oplog")
            .with_status(200)
            .with_body("not-a-loro-snapshot")
            .expect(1)
            .create_async()
            .await;
        let put_oplog = server
            .mock("PUT", "/aurora/note-1.oplog")
            .expect(0)
            .create_async()
            .await;
        let put_index = server
            .mock("PUT", "/aurora/index.json")
            .expect(0)
            .create_async()
            .await;
        let mut target = WebDavTarget::new();
        let conn = connect_to(&server, &mut target).await;

        let err = target
            .push_with_merge(&conn, "note-1", b"also-not-loro", 1)
            .await
            .unwrap_err();
        assert!(matches!(err, aurora_core::Error::SyncConflict { .. }));
        index.assert();
        oplog.assert();
        put_oplog.assert();
        put_index.assert();
    }
}
