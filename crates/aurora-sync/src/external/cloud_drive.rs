//! 云盘同步 (Cloud Drive Sync)
//!
//! 支持 WebDAV 与 Google Drive / Dropbox / OneDrive 四类云盘提供商。
//! - [`CloudDriveConnector`] trait 统一文件操作 (list / download / upload / delete)。
//! - [`SelectiveSyncConfig`] 选择性同步：路径包含 / 排除、大小上限、隐藏文件。
//! - [`CloudDriveSync`] 双向同步引擎：基于 ETag 增量检测 + LWW 冲突解决。
//!
//! # 实现说明
//! 四个连接器均为内存 mock (共享 [`MockDrive`] 存储)，真实实现替换
//! `upload` / `download` / `list_files` 为对应 HTTP API 即可，公开 API 不变。

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use super::{ConnectorState, SyncConnector, SyncSession};

/// 云盘提供商。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum DriveProvider {
    /// WebDAV (RFC 4918)。
    WebDav,
    /// Google Drive。
    GoogleDrive,
    /// Dropbox。
    Dropbox,
    /// Microsoft OneDrive。
    OneDrive,
}

impl DriveProvider {
    /// 提供商字符串标识 (用于 SyncConnector::provider)。
    pub fn as_str(&self) -> &'static str {
        match self {
            DriveProvider::WebDav => "webdav",
            DriveProvider::GoogleDrive => "google_drive",
            DriveProvider::Dropbox => "dropbox",
            DriveProvider::OneDrive => "one_drive",
        }
    }
}

/// 云盘文件元数据。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriveFile {
    /// 文件 ID (提供商侧唯一)。
    pub id: String,
    /// 文件名 (不含路径)。
    pub name: String,
    /// 完整路径 (如 `/docs/note.md`)。
    pub path: String,
    /// 字节大小。
    pub size: usize,
    /// MIME 类型。
    pub mime_type: String,
    /// ETag (版本指纹)。
    pub etag: String,
    /// 最后修改时间。
    pub modified: chrono::DateTime<chrono::Utc>,
    /// 是否为目录。
    pub is_dir: bool,
    /// 所属提供商。
    pub provider: DriveProvider,
}

impl DriveFile {
    pub fn new(path: impl Into<String>, size: usize, provider: DriveProvider) -> Self {
        let path = path.into();
        let name = path.rsplit('/').next().unwrap_or(&path).to_string();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            path,
            size,
            mime_type: String::new(),
            etag: uuid::Uuid::new_v4().to_string(),
            modified: chrono::Utc::now(),
            is_dir: false,
            provider,
        }
    }
}

/// 选择性同步配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectiveSyncConfig {
    /// 包含路径前缀 (为空表示不限)。
    pub included_paths: Vec<String>,
    /// 排除路径前缀。
    pub excluded_paths: Vec<String>,
    /// 单文件大小上限 (字节)，None 表示不限。
    pub max_file_size: Option<usize>,
    /// 是否同步隐藏文件 (以 `.` 开头)。
    pub sync_hidden: bool,
}

impl Default for SelectiveSyncConfig {
    fn default() -> Self {
        Self {
            included_paths: Vec::new(),
            excluded_paths: Vec::new(),
            max_file_size: None,
            sync_hidden: true,
        }
    }
}

impl SelectiveSyncConfig {
    /// 判断文件是否在选择性同步范围内。
    pub fn is_included(&self, file: &DriveFile) -> bool {
        // 隐藏文件
        if !self.sync_hidden && file.name.starts_with('.') {
            return false;
        }
        // 大小上限
        if let Some(max) = self.max_file_size {
            if file.size > max {
                return false;
            }
        }
        // 排除路径
        for ex in &self.excluded_paths {
            if path_under(&file.path, ex) {
                return false;
            }
        }
        // 包含路径
        if !self.included_paths.is_empty() {
            let mut ok = false;
            for inc in &self.included_paths {
                if path_under(&file.path, inc) {
                    ok = true;
                    break;
                }
            }
            if !ok {
                return false;
            }
        }
        true
    }
}

/// 判断 `path` 是否等于或位于 `prefix` 之下。
fn path_under(path: &str, prefix: &str) -> bool {
    if prefix.is_empty() {
        return true;
    }
    if path == prefix {
        return true;
    }
    let prefix_norm = prefix.trim_end_matches('/');
    path.starts_with(prefix_norm) && path[prefix_norm.len()..].starts_with('/')
}

/// 根据扩展名猜测 MIME 类型。
fn guess_mime(path: &str) -> String {
    let ext = path.rsplit('.').next().unwrap_or("");
    match ext.to_lowercase().as_str() {
        "txt" | "md" => "text/plain",
        "pdf" => "application/pdf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "json" => "application/json",
        "html" | "htm" => "text/html",
        _ => "application/octet-stream",
    }
    .to_string()
}

/// 云盘连接器 trait (文件操作抽象)。
pub trait CloudDriveConnector: Send + Sync {
    /// 提供商类型。
    fn drive_provider(&self) -> DriveProvider;
    /// 列出指定路径前缀下的文件。
    fn list_files(&self, path: &str) -> crate::Result<Vec<DriveFile>>;
    /// 下载文件内容。
    fn download(&self, id: &str) -> crate::Result<Vec<u8>>;
    /// 上传文件，返回元数据。
    fn upload(&self, path: &str, data: Vec<u8>) -> crate::Result<DriveFile>;
    /// 删除文件，返回是否实际删除。
    fn delete(&self, id: &str) -> crate::Result<bool>;
    /// 获取文件 ETag。
    fn get_etag(&self, id: &str) -> Option<String>;
    /// 文件总数。
    fn file_count(&self) -> usize;
}

/// 内部共享的 mock 云盘存储。
struct MockDrive {
    name: String,
    provider: DriveProvider,
    state: Arc<RwLock<ConnectorState>>,
    /// id -> 文件元数据。
    files: Arc<RwLock<HashMap<String, DriveFile>>>,
    /// id -> 内容。
    contents: Arc<RwLock<HashMap<String, Vec<u8>>>>,
    /// path -> id 索引 (用于按路径上传覆盖)。
    path_index: Arc<RwLock<HashMap<String, String>>>,
}

impl MockDrive {
    fn new(name: impl Into<String>, provider: DriveProvider) -> Self {
        Self {
            name: name.into(),
            provider,
            state: Arc::new(RwLock::new(ConnectorState::Disconnected)),
            files: Arc::new(RwLock::new(HashMap::new())),
            contents: Arc::new(RwLock::new(HashMap::new())),
            path_index: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn list_files(&self, prefix: &str) -> crate::Result<Vec<DriveFile>> {
        let files = self.files.read();
        let mut result: Vec<DriveFile> = files
            .values()
            .filter(|f| prefix.is_empty() || path_under(&f.path, prefix))
            .cloned()
            .collect();
        result.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(result)
    }

    fn download(&self, id: &str) -> crate::Result<Vec<u8>> {
        self.contents
            .read()
            .get(id)
            .cloned()
            .ok_or_else(|| crate::Error::NotFound(format!("drive file not found: {}", id)))
    }

    fn upload(&self, path: &str, data: Vec<u8>) -> crate::Result<DriveFile> {
        let size = data.len();
        // 若路径已存在，复用 id 但刷新 etag；否则新建 id。
        let id = self
            .path_index
            .read()
            .get(path)
            .cloned()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let file = DriveFile {
            id: id.clone(),
            name: path.rsplit('/').next().unwrap_or(path).to_string(),
            path: path.to_string(),
            size,
            mime_type: guess_mime(path),
            etag: uuid::Uuid::new_v4().to_string(),
            modified: chrono::Utc::now(),
            is_dir: false,
            provider: self.provider,
        };
        self.contents.write().insert(id.clone(), data);
        self.path_index.write().insert(path.to_string(), id.clone());
        self.files.write().insert(id, file.clone());
        debug!("drive upload: path={} size={}", path, size);
        Ok(file)
    }

    fn delete(&self, id: &str) -> crate::Result<bool> {
        let mut files = self.files.write();
        if let Some(file) = files.remove(id) {
            self.path_index.write().remove(&file.path);
            self.contents.write().remove(id);
            debug!("drive delete: id={}", id);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn get_etag(&self, id: &str) -> Option<String> {
        self.files.read().get(id).map(|f| f.etag.clone())
    }

    fn file_count(&self) -> usize {
        self.files.read().len()
    }

    fn connect(&self) -> crate::Result<()> {
        *self.state.write() = ConnectorState::Connecting;
        *self.state.write() = ConnectorState::Connected;
        Ok(())
    }

    fn disconnect(&self) -> crate::Result<()> {
        *self.state.write() = ConnectorState::Disconnected;
        Ok(())
    }

    fn state(&self) -> ConnectorState {
        self.state.read().clone()
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// 为各 mock 云盘连接器生成结构体与 trait 实现 (CloudDriveConnector + SyncConnector)。
macro_rules! impl_mock_drive_connector {
    ($struct_name:ident, $provider:expr) => {
        pub struct $struct_name {
            drive: MockDrive,
        }

        impl $struct_name {
            pub fn new(name: impl Into<String>) -> Self {
                Self {
                    drive: MockDrive::new(name, $provider),
                }
            }

            /// 测试辅助：直接注入文件 (mock 服务端)。
            pub fn seed(&self, path: &str, data: Vec<u8>) -> DriveFile {
                CloudDriveConnector::upload(self, path, data).expect("seed upload")
            }
        }

        impl CloudDriveConnector for $struct_name {
            fn drive_provider(&self) -> DriveProvider {
                $provider
            }
            fn list_files(&self, path: &str) -> crate::Result<Vec<DriveFile>> {
                self.drive.list_files(path)
            }
            fn download(&self, id: &str) -> crate::Result<Vec<u8>> {
                self.drive.download(id)
            }
            fn upload(&self, path: &str, data: Vec<u8>) -> crate::Result<DriveFile> {
                self.drive.upload(path, data)
            }
            fn delete(&self, id: &str) -> crate::Result<bool> {
                self.drive.delete(id)
            }
            fn get_etag(&self, id: &str) -> Option<String> {
                self.drive.get_etag(id)
            }
            fn file_count(&self) -> usize {
                self.drive.file_count()
            }
        }

        impl SyncConnector for $struct_name {
            fn name(&self) -> &str {
                self.drive.name()
            }
            fn provider(&self) -> &str {
                ($provider).as_str()
            }
            fn connect(&self) -> crate::Result<()> {
                self.drive.connect()
            }
            fn disconnect(&self) -> crate::Result<()> {
                self.drive.disconnect()
            }
            fn sync(&self) -> crate::Result<SyncSession> {
                if !self.drive.state().is_connected() {
                    return Err(crate::Error::ExternalSync(format!(
                        "drive connector not connected: {}",
                        self.drive.name()
                    )));
                }
                let count = self.drive.file_count();
                let mut session =
                    SyncSession::new(self.drive.name().to_string(), ($provider).as_str());
                session.finish(count, 0);
                Ok(session)
            }
            fn state(&self) -> ConnectorState {
                self.drive.state()
            }
        }
    };
}

impl_mock_drive_connector!(WebDavConnector, DriveProvider::WebDav);
impl_mock_drive_connector!(GoogleDriveConnector, DriveProvider::GoogleDrive);
impl_mock_drive_connector!(DropboxConnector, DriveProvider::Dropbox);
impl_mock_drive_connector!(OneDriveConnector, DriveProvider::OneDrive);

/// 双向同步冲突描述。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriveConflict {
    pub path: String,
    pub local_etag: Option<String>,
    pub remote_etag: Option<String>,
}

/// 远端变更集合。
#[derive(Debug, Clone, Default)]
pub struct DriveChangeSet {
    pub updated: Vec<DriveFile>,
    pub deleted: Vec<String>,
}

impl DriveChangeSet {
    pub fn is_empty(&self) -> bool {
        self.updated.is_empty() && self.deleted.is_empty()
    }
}

/// 云盘双向同步引擎。
///
/// 维护本地文件副本与 ETag，执行选择性同步 + LWW 冲突解决。
pub struct CloudDriveSync {
    connector: Arc<dyn CloudDriveConnector>,
    config: SelectiveSyncConfig,
    /// path -> 本地文件元数据。
    local_files: Arc<RwLock<HashMap<String, DriveFile>>>,
    /// path -> 本地内容缓存。
    local_contents: Arc<RwLock<HashMap<String, Vec<u8>>>>,
    /// path -> 上次同步记录的远端 ETag。
    local_etags: Arc<RwLock<HashMap<String, String>>>,
    /// 自上次同步以来本地修改过的 path。
    locally_modified: Arc<RwLock<HashSet<String>>>,
}

impl CloudDriveSync {
    pub fn new(connector: Arc<dyn CloudDriveConnector>, config: SelectiveSyncConfig) -> Self {
        Self {
            connector,
            config,
            local_files: Arc::new(RwLock::new(HashMap::new())),
            local_contents: Arc::new(RwLock::new(HashMap::new())),
            local_etags: Arc::new(RwLock::new(HashMap::new())),
            locally_modified: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    /// 本地写入 / 更新文件 (标记本地修改)。
    pub fn local_put(&self, path: &str, data: Vec<u8>) {
        let provider = self.connector.drive_provider();
        let mut file = DriveFile::new(path, data.len(), provider);
        file.etag = format!("local-{}", uuid::Uuid::new_v4());
        self.local_contents.write().insert(path.to_string(), data);
        self.local_files.write().insert(path.to_string(), file);
        self.locally_modified.write().insert(path.to_string());
    }

    /// 本地删除文件 (标记本地修改)。
    pub fn local_delete(&self, path: &str) -> bool {
        let removed = self.local_files.write().remove(path).is_some();
        self.local_contents.write().remove(path);
        if removed {
            self.locally_modified.write().insert(path.to_string());
        }
        removed
    }

    /// 本地文件数。
    pub fn local_count(&self) -> usize {
        self.local_files.read().len()
    }

    /// 获取本地文件。
    pub fn local_get(&self, path: &str) -> Option<DriveFile> {
        self.local_files.read().get(path).cloned()
    }

    /// 获取本地文件内容。
    pub fn local_content(&self, path: &str) -> Option<Vec<u8>> {
        self.local_contents.read().get(path).cloned()
    }

    /// 增量检测远端变更 (应用选择性同步过滤)。
    pub fn detect_remote_changes(&self) -> crate::Result<DriveChangeSet> {
        let remote_files = self.connector.list_files("")?;
        let mut remote_map: HashMap<String, DriveFile> = HashMap::new();
        for f in remote_files {
            if self.config.is_included(&f) {
                remote_map.insert(f.path.clone(), f);
            }
        }
        let known_etags = self.local_etags.read();
        let mut updated = Vec::new();
        for (path, file) in &remote_map {
            match known_etags.get(path) {
                Some(t) if t == &file.etag => {} // 未变
                _ => updated.push(file.clone()),
            }
        }
        // 远端已删除：本地有记录但远端不再存在
        let mut deleted = Vec::new();
        for path in known_etags.keys() {
            if !remote_map.contains_key(path) {
                deleted.push(path.clone());
            }
        }
        Ok(DriveChangeSet { updated, deleted })
    }

    /// 检测双向冲突：同时被本地与远端修改的路径。
    pub fn detect_conflicts(&self) -> crate::Result<Vec<DriveConflict>> {
        let changes = self.detect_remote_changes()?;
        let modified = self.locally_modified.read();
        let known_etags = self.local_etags.read();
        let mut conflicts = Vec::new();
        for f in &changes.updated {
            if modified.contains(&f.path) {
                let local_etag = self
                    .local_files
                    .read()
                    .get(&f.path)
                    .map(|lf| lf.etag.clone());
                conflicts.push(DriveConflict {
                    path: f.path.clone(),
                    local_etag,
                    remote_etag: Some(f.etag.clone()),
                });
            }
        }
        // 本地删除但远端更新也是一种冲突
        for path in modified.iter() {
            if !self.local_files.read().contains_key(path)
                && known_etags.contains_key(path)
                && changes.updated.iter().any(|f| &f.path == path)
            {
                conflicts.push(DriveConflict {
                    path: path.clone(),
                    local_etag: None,
                    remote_etag: changes
                        .updated
                        .iter()
                        .find(|f| &f.path == path)
                        .map(|f| f.etag.clone()),
                });
            }
        }
        Ok(conflicts)
    }

    /// 拉取远端变更到本地。
    pub fn pull_remote(&self) -> crate::Result<usize> {
        let changes = self.detect_remote_changes()?;
        let mut applied = 0;
        for f in &changes.updated {
            let content = self.connector.download(&f.id)?;
            self.local_files.write().insert(f.path.clone(), f.clone());
            self.local_contents.write().insert(f.path.clone(), content);
            self.local_etags
                .write()
                .insert(f.path.clone(), f.etag.clone());
            applied += 1;
        }
        for path in &changes.deleted {
            self.local_files.write().remove(path);
            self.local_contents.write().remove(path);
            self.local_etags.write().remove(path);
            applied += 1;
        }
        if applied > 0 {
            info!("drive pull_remote: applied={} changes", applied);
        }
        Ok(applied)
    }

    /// 推送本地修改到远端。
    pub fn push_local(&self) -> crate::Result<usize> {
        let to_push: Vec<(String, Vec<u8>)> = {
            let modified = self.locally_modified.read();
            let contents = self.local_contents.read();
            modified
                .iter()
                .filter_map(|p| contents.get(p).map(|c| (p.clone(), c.clone())))
                .collect()
        };
        let to_delete: Vec<String> = {
            let modified = self.locally_modified.read();
            let files = self.local_files.read();
            modified
                .iter()
                .filter(|p| !files.contains_key(*p))
                .cloned()
                .collect()
        };
        let mut pushed = 0;
        for (path, data) in &to_push {
            let file = self.connector.upload(path, data.clone())?;
            self.local_files.write().insert(path.clone(), file.clone());
            self.local_etags
                .write()
                .insert(path.clone(), file.etag.clone());
            pushed += 1;
        }
        // 删除：按 path 找到远端 id
        for path in &to_delete {
            let remote_files = self.connector.list_files("")?;
            if let Some(rf) = remote_files.iter().find(|f| &f.path == path) {
                self.connector.delete(&rf.id)?;
            }
            self.local_etags.write().remove(path);
            pushed += 1;
        }
        self.locally_modified.write().clear();
        if pushed > 0 {
            info!("drive push_local: pushed={} items", pushed);
        }
        Ok(pushed)
    }

    /// 完整双向同步：LWW 解决冲突。
    ///
    /// 返回 (同步条目数, 冲突数)。
    pub fn full_sync(&self) -> crate::Result<(usize, usize)> {
        let conflicts = self.detect_conflicts()?;
        let conflict_count = conflicts.len();
        for c in &conflicts {
            let local = self.local_files.read().get(&c.path).cloned();
            let remote_files = self.connector.list_files("")?;
            let remote = remote_files.iter().find(|f| f.path == c.path).cloned();
            match (local, remote) {
                (Some(l), Some(r)) => {
                    if l.modified >= r.modified {
                        // 本地较新，推送覆盖远端
                        if let Some(content) = self.local_contents.read().get(&c.path).cloned() {
                            self.connector.upload(&c.path, content)?;
                        }
                    } else {
                        // 远端较新，拉取覆盖本地
                        let content = self.connector.download(&r.id)?;
                        self.local_files.write().insert(c.path.clone(), r);
                        self.local_contents.write().insert(c.path.clone(), content);
                    }
                }
                (Some(l), None) => {
                    // 远端已删除但本地仍修改 → 推送本地 (恢复)
                    if let Some(content) = self.local_contents.read().get(&c.path).cloned() {
                        self.connector.upload(&l.path, content)?;
                    }
                }
                _ => {}
            }
            self.locally_modified.write().remove(&c.path);
        }
        if conflict_count > 0 {
            warn!(
                "drive full_sync: resolved {} conflicts (LWW)",
                conflict_count
            );
        }
        let pushed = self.push_local()?;
        let pulled = self.pull_remote()?;
        Ok((pushed + pulled, conflict_count))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_webdav() -> WebDavConnector {
        WebDavConnector::new("drive")
    }

    #[test]
    fn test_drive_provider_as_str() {
        assert_eq!(DriveProvider::WebDav.as_str(), "webdav");
        assert_eq!(DriveProvider::GoogleDrive.as_str(), "google_drive");
        assert_eq!(DriveProvider::Dropbox.as_str(), "dropbox");
        assert_eq!(DriveProvider::OneDrive.as_str(), "one_drive");
    }

    #[test]
    fn test_drive_file_new_extracts_name() {
        let f = DriveFile::new("/docs/note.md", 10, DriveProvider::WebDav);
        assert_eq!(f.name, "note.md");
        assert_eq!(f.path, "/docs/note.md");
        assert_eq!(f.provider, DriveProvider::WebDav);
        assert!(!f.is_dir);
    }

    #[test]
    fn test_path_under() {
        assert!(path_under("/docs/a.md", "/docs"));
        assert!(path_under("/docs/a.md", "/docs/"));
        assert!(path_under("/docs", "/docs"));
        assert!(path_under("/docs/sub/a.md", "/docs"));
        assert!(!path_under("/other/a.md", "/docs"));
        assert!(path_under("/any", "")); // 空前缀匹配一切
    }

    #[test]
    fn test_selective_sync_included_paths() {
        let cfg = SelectiveSyncConfig {
            included_paths: vec!["/docs".to_string()],
            excluded_paths: vec![],
            max_file_size: None,
            sync_hidden: true,
        };
        let in_docs = DriveFile::new("/docs/a.md", 5, DriveProvider::WebDav);
        let in_pics = DriveFile::new("/pics/a.png", 5, DriveProvider::WebDav);
        assert!(cfg.is_included(&in_docs));
        assert!(!cfg.is_included(&in_pics));
    }

    #[test]
    fn test_selective_sync_excluded_paths() {
        let cfg = SelectiveSyncConfig {
            included_paths: vec![],
            excluded_paths: vec!["/tmp".to_string()],
            max_file_size: None,
            sync_hidden: true,
        };
        let tmp_file = DriveFile::new("/tmp/x.md", 5, DriveProvider::WebDav);
        let docs_file = DriveFile::new("/docs/y.md", 5, DriveProvider::WebDav);
        assert!(!cfg.is_included(&tmp_file));
        assert!(cfg.is_included(&docs_file));
    }

    #[test]
    fn test_selective_sync_max_size_and_hidden() {
        let cfg = SelectiveSyncConfig {
            included_paths: vec![],
            excluded_paths: vec![],
            max_file_size: Some(100),
            sync_hidden: false,
        };
        let big = DriveFile::new("/docs/big.bin", 200, DriveProvider::WebDav);
        let hidden = DriveFile::new("/docs/.secret", 10, DriveProvider::WebDav);
        let ok = DriveFile::new("/docs/ok.md", 50, DriveProvider::WebDav);
        assert!(!cfg.is_included(&big));
        assert!(!cfg.is_included(&hidden));
        assert!(cfg.is_included(&ok));
    }

    #[test]
    fn test_webdav_upload_download_list() {
        let conn = make_webdav();
        let f1 = conn.upload("/docs/a.md", vec![1, 2, 3]).unwrap();
        let _f2 = conn.upload("/docs/b.md", vec![4, 5]).unwrap();
        assert_eq!(conn.file_count(), 2);
        // list
        let listed = conn.list_files("/docs").unwrap();
        assert_eq!(listed.len(), 2);
        // download
        let data = conn.download(&f1.id).unwrap();
        assert_eq!(data, vec![1, 2, 3]);
        // mime 猜测
        assert_eq!(f1.mime_type, "text/plain");
    }

    #[test]
    fn test_webdav_upload_same_path_refreshes_etag() {
        let conn = make_webdav();
        let f1 = conn.upload("/docs/a.md", vec![1]).unwrap();
        let f2 = conn.upload("/docs/a.md", vec![1, 2]).unwrap();
        assert_eq!(conn.file_count(), 1); // 路径覆盖，非新增
        assert_ne!(f1.etag, f2.etag); // ETag 刷新
        assert_eq!(f2.size, 2);
    }

    #[test]
    fn test_webdav_delete() {
        let conn = make_webdav();
        let f = conn.upload("/docs/a.md", vec![1]).unwrap();
        assert!(conn.delete(&f.id).unwrap());
        assert_eq!(conn.file_count(), 0);
        assert!(!conn.delete(&f.id).unwrap()); // 再次删除返回 false
        assert!(conn.download(&f.id).is_err());
    }

    #[test]
    fn test_webdav_get_etag() {
        let conn = make_webdav();
        let f = conn.upload("/docs/a.md", vec![1]).unwrap();
        assert_eq!(conn.get_etag(&f.id), Some(f.etag));
        assert_eq!(conn.get_etag("nonexistent"), None);
    }

    #[test]
    fn test_webdav_sync_connector_impl() {
        use super::super::SyncConnector;
        let conn = make_webdav();
        assert_eq!(conn.provider(), "webdav");
        assert_eq!(conn.state(), ConnectorState::Disconnected);
        conn.connect().unwrap();
        assert_eq!(conn.state(), ConnectorState::Connected);
        conn.upload("/a.md", vec![1, 2]).unwrap();
        let session = conn.sync().unwrap();
        assert_eq!(session.items_synced, 1);
        conn.disconnect().unwrap();
        assert!(conn.sync().is_err()); // 断开后报错
    }

    #[test]
    fn test_all_providers_distinct() {
        let wd = WebDavConnector::new("wd");
        let gd = GoogleDriveConnector::new("gd");
        let db = DropboxConnector::new("db");
        let od = OneDriveConnector::new("od");
        assert_eq!(wd.drive_provider(), DriveProvider::WebDav);
        assert_eq!(gd.drive_provider(), DriveProvider::GoogleDrive);
        assert_eq!(db.drive_provider(), DriveProvider::Dropbox);
        assert_eq!(od.drive_provider(), DriveProvider::OneDrive);
        // 各自独立存储
        wd.upload("/a.md", vec![1]).unwrap();
        assert_eq!(wd.file_count(), 1);
        assert_eq!(gd.file_count(), 0);
    }

    #[test]
    fn test_sync_pull_remote() {
        let conn = Arc::new(make_webdav());
        let cfg = SelectiveSyncConfig::default();
        let sync = CloudDriveSync::new(conn.clone() as Arc<dyn CloudDriveConnector>, cfg);
        conn.upload("/docs/a.md", vec![1, 2, 3]).unwrap();
        conn.upload("/docs/b.md", vec![4, 5]).unwrap();
        let applied = sync.pull_remote().unwrap();
        assert_eq!(applied, 2);
        assert_eq!(sync.local_count(), 2);
        assert_eq!(sync.local_content("/docs/a.md"), Some(vec![1, 2, 3]));
    }

    #[test]
    fn test_sync_pull_incremental_etag() {
        let conn = Arc::new(make_webdav());
        let sync = CloudDriveSync::new(
            conn.clone() as Arc<dyn CloudDriveConnector>,
            SelectiveSyncConfig::default(),
        );
        conn.upload("/docs/a.md", vec![1]).unwrap();
        sync.pull_remote().unwrap();
        // 无变化
        let applied = sync.pull_remote().unwrap();
        assert_eq!(applied, 0);
        // 修改远端文件 (刷新 etag)
        conn.upload("/docs/a.md", vec![1, 2, 3]).unwrap();
        let applied2 = sync.pull_remote().unwrap();
        assert_eq!(applied2, 1);
        assert_eq!(sync.local_content("/docs/a.md"), Some(vec![1, 2, 3]));
    }

    #[test]
    fn test_sync_push_local() {
        let conn = Arc::new(make_webdav());
        let sync = CloudDriveSync::new(
            conn.clone() as Arc<dyn CloudDriveConnector>,
            SelectiveSyncConfig::default(),
        );
        sync.local_put("/docs/x.md", vec![9, 9]);
        let pushed = sync.push_local().unwrap();
        assert_eq!(pushed, 1);
        assert_eq!(conn.file_count(), 1);
        // 再次推送无变化
        let pushed2 = sync.push_local().unwrap();
        assert_eq!(pushed2, 0);
    }

    #[test]
    fn test_sync_push_local_deletion() {
        let conn = Arc::new(make_webdav());
        let sync = CloudDriveSync::new(
            conn.clone() as Arc<dyn CloudDriveConnector>,
            SelectiveSyncConfig::default(),
        );
        sync.local_put("/docs/x.md", vec![1]);
        sync.push_local().unwrap();
        assert_eq!(conn.file_count(), 1);
        sync.local_delete("/docs/x.md");
        let pushed = sync.push_local().unwrap();
        assert_eq!(pushed, 1);
        assert_eq!(conn.file_count(), 0);
    }

    #[test]
    fn test_sync_selective_filter_applied() {
        let conn = Arc::new(make_webdav());
        let cfg = SelectiveSyncConfig {
            included_paths: vec!["/docs".to_string()],
            excluded_paths: vec![],
            max_file_size: None,
            sync_hidden: true,
        };
        let sync = CloudDriveSync::new(conn.clone() as Arc<dyn CloudDriveConnector>, cfg);
        conn.upload("/docs/a.md", vec![1]).unwrap();
        conn.upload("/pics/b.png", vec![2]).unwrap();
        let applied = sync.pull_remote().unwrap();
        assert_eq!(applied, 1); // 仅 /docs 被拉取
        assert!(sync.local_get("/docs/a.md").is_some());
        assert!(sync.local_get("/pics/b.png").is_none());
    }

    #[test]
    fn test_sync_detect_conflicts_both_modified() {
        let conn = Arc::new(make_webdav());
        let sync = CloudDriveSync::new(
            conn.clone() as Arc<dyn CloudDriveConnector>,
            SelectiveSyncConfig::default(),
        );
        conn.upload("/docs/a.md", vec![1]).unwrap();
        sync.pull_remote().unwrap();
        // 双方都修改
        conn.upload("/docs/a.md", vec![2]).unwrap();
        sync.local_put("/docs/a.md", vec![3]);
        let conflicts = sync.detect_conflicts().unwrap();
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].path, "/docs/a.md");
    }

    #[test]
    fn test_sync_full_sync_lww_local_wins() {
        let conn = Arc::new(make_webdav());
        let sync = CloudDriveSync::new(
            conn.clone() as Arc<dyn CloudDriveConnector>,
            SelectiveSyncConfig::default(),
        );
        conn.upload("/docs/a.md", vec![1]).unwrap();
        sync.pull_remote().unwrap();
        // 远端修改 (较早)
        let mut remote = conn.upload("/docs/a.md", vec![2]).unwrap();
        remote.modified = chrono::Utc::now() - chrono::Duration::hours(2);
        // 本地修改 (较晚) → LWW 选本地
        sync.local_put("/docs/a.md", vec![3]);
        let (synced, conflicts) = sync.full_sync().unwrap();
        assert_eq!(conflicts, 1);
        assert!(synced > 0);
        // 远端最终内容应为本地版本
        let remote_files = conn.list_files("").unwrap();
        let rf = remote_files
            .iter()
            .find(|f| f.path == "/docs/a.md")
            .unwrap();
        assert_eq!(conn.download(&rf.id).unwrap(), vec![3]);
    }

    #[test]
    fn test_changeset_empty() {
        let cs = DriveChangeSet::default();
        assert!(cs.is_empty());
    }

    #[test]
    fn test_registry_integration_with_webdav() {
        use super::super::{ConnectorRegistry, SyncConnector};
        let conn = Arc::new(make_webdav());
        let reg = ConnectorRegistry::new();
        reg.register("drive", conn.clone() as Arc<dyn SyncConnector>)
            .unwrap();
        reg.connect("drive").unwrap();
        assert_eq!(reg.state("drive"), Some(ConnectorState::Connected));
        let s = reg.sync("drive").unwrap();
        assert_eq!(s.provider, "webdav");
    }
}
