//! Aurora Note 实体定义 (Entity Layer)
//!
//! 对应 V19 §29 Schema 定义，提供所有持久化实体的 Rust 结构体表示。
//! 每个实体均实现 `serde::Serialize/Deserialize` 以及与 `rusqlite` 的互转辅助方法。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ── 核心数据实体 ───────────────────────────────────────────────

/// 笔记实体 (对应 `notes` 表)。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Note {
    pub id: String,
    pub title: String,
    pub content: String,
    pub content_type: String, // e.g. "markdown", "html", "loro"
    pub workspace_id: Option<String>,
    pub parent_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub version: i64,
    pub loro_doc_id: Option<String>,
    pub metadata: serde_json::Value,
    // ── V19 §11 设计字段（v2 迁移补齐，DEV-003） ──
    /// 内容文件系统路径（外部同步模式；空 = content 内联）。
    #[serde(default)]
    pub file_path: String,
    /// 内容 SHA-256 校验和（外部同步完整性）。
    #[serde(default)]
    pub file_hash: Option<String>,
    /// Lamport 时间戳（外部同步冲突解决）。
    #[serde(default)]
    pub lamport_ts: i64,
    /// 同步状态: syncing | synced | conflict。
    #[serde(default = "default_sync_state")]
    pub sync_state: String,
    /// 加密级别: none | shared | private（工作区分级）。
    #[serde(default = "default_encryption")]
    pub encryption: String,
    /// 软删除标志（V19 命名；与 deleted_at 并存，语义一致）。
    #[serde(default)]
    pub is_deleted: bool,
}

fn default_sync_state() -> String {
    "synced".into()
}

fn default_encryption() -> String {
    "none".into()
}

impl Note {
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: id.into(),
            title: title.into(),
            content: String::new(),
            content_type: "markdown".into(),
            workspace_id: None,
            parent_id: None,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            version: 1,
            loro_doc_id: None,
            metadata: serde_json::Value::Object(Default::default()),
            file_path: String::new(),
            file_hash: None,
            lamport_ts: 0,
            sync_state: "synced".into(),
            encryption: "none".into(),
            is_deleted: false,
        }
    }
}

/// 任务实体 (对应 `tasks` 表)。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub status: TaskStatus,
    pub priority: i32,
    pub due_date: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub note_id: Option<String>,
    pub workspace_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskStatus {
    #[serde(rename = "todo")]
    Todo,
    #[serde(rename = "in_progress")]
    InProgress,
    #[serde(rename = "done")]
    Done,
    #[serde(rename = "cancelled")]
    Cancelled,
}

impl Task {
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: id.into(),
            title: title.into(),
            description: None,
            status: TaskStatus::Todo,
            priority: 0,
            due_date: None,
            completed_at: None,
            note_id: None,
            workspace_id: None,
            created_at: now,
            updated_at: now,
            metadata: serde_json::Value::Object(Default::default()),
        }
    }
}

/// 双向链接实体 (对应 `links` 表)。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Link {
    pub id: String,
    pub source_note_id: String,
    pub target_note_id: String,
    pub link_type: String,
    pub created_at: DateTime<Utc>,
    pub metadata: serde_json::Value,
}

impl Link {
    pub fn new(
        id: impl Into<String>,
        source: impl Into<String>,
        target: impl Into<String>,
        link_type: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            source_note_id: source.into(),
            target_note_id: target.into(),
            link_type: link_type.into(),
            created_at: Utc::now(),
            metadata: serde_json::Value::Object(Default::default()),
        }
    }
}

/// 工作空间实体 (对应 `workspaces` 表)。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub metadata: serde_json::Value,
}

impl Workspace {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: id.into(),
            name: name.into(),
            description: None,
            created_at: now,
            updated_at: now,
            metadata: serde_json::Value::Object(Default::default()),
        }
    }
}

// ── 事件与审计实体 ─────────────────────────────────────────────

/// 事件队列记录 (对应 `event_queue` 表)。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EventRecord {
    pub id: i64,
    pub channel: String, // high / medium / low
    pub event_type: String,
    pub payload: serde_json::Value,
    pub seq: i64,
    pub created_at: DateTime<Utc>,
    pub consumed_at: Option<DateTime<Utc>>,
}

/// 审计日志实体 (对应 `audit_log` 表)。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuditLog {
    pub id: i64,
    pub actor: String,
    pub action: String,
    pub resource_type: String,
    pub resource_id: String,
    pub details: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

/// 版本快照实体 (对应 `version_snapshots` 表)。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VersionSnapshot {
    pub id: i64,
    pub note_id: String,
    pub snapshot_data: Vec<u8>,
    pub version: i64,
    pub created_at: DateTime<Utc>,
}

/// 写前日志记录 (对应 `pending_writes` 表)。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PendingWriteRecord {
    pub id: i64,
    pub file_path: String,
    pub tmp_path: String,
    pub loro_op_id: String,
    pub checksum: Option<String>,
    pub created_at: DateTime<Utc>,
}

// ── rusqlite 互转辅助 trait ───────────────────────────────────

/// 将实体转换为 rusqlite 参数列表。
pub trait ToSqlParams {
    fn to_params(&self) -> Vec<Box<dyn rusqlite::ToSql>>;
}

impl ToSqlParams for Note {
    fn to_params(&self) -> Vec<Box<dyn rusqlite::ToSql>> {
        vec![
            Box::new(self.id.clone()),
            Box::new(self.title.clone()),
            Box::new(self.content.clone()),
            Box::new(self.content_type.clone()),
            Box::new(self.workspace_id.clone()),
            Box::new(self.parent_id.clone()),
            Box::new(self.created_at.to_rfc3339()),
            Box::new(self.updated_at.to_rfc3339()),
            Box::new(self.deleted_at.map(|d| d.to_rfc3339())),
            Box::new(self.version),
            Box::new(self.loro_doc_id.clone()),
            Box::new(serde_json::to_string(&self.metadata).unwrap_or_default()),
        ]
    }
}

impl ToSqlParams for Task {
    fn to_params(&self) -> Vec<Box<dyn rusqlite::ToSql>> {
        vec![
            Box::new(self.id.clone()),
            Box::new(self.title.clone()),
            Box::new(self.description.clone()),
            Box::new(format!("{:?}", self.status).to_lowercase()),
            Box::new(self.priority),
            Box::new(self.due_date.map(|d| d.to_rfc3339())),
            Box::new(self.completed_at.map(|d| d.to_rfc3339())),
            Box::new(self.note_id.clone()),
            Box::new(self.workspace_id.clone()),
            Box::new(self.created_at.to_rfc3339()),
            Box::new(self.updated_at.to_rfc3339()),
            Box::new(serde_json::to_string(&self.metadata).unwrap_or_default()),
        ]
    }
}

impl ToSqlParams for Link {
    fn to_params(&self) -> Vec<Box<dyn rusqlite::ToSql>> {
        vec![
            Box::new(self.id.clone()),
            Box::new(self.source_note_id.clone()),
            Box::new(self.target_note_id.clone()),
            Box::new(self.link_type.clone()),
            Box::new(self.created_at.to_rfc3339()),
            Box::new(serde_json::to_string(&self.metadata).unwrap_or_default()),
        ]
    }
}

impl ToSqlParams for Workspace {
    fn to_params(&self) -> Vec<Box<dyn rusqlite::ToSql>> {
        vec![
            Box::new(self.id.clone()),
            Box::new(self.name.clone()),
            Box::new(self.description.clone()),
            Box::new(self.created_at.to_rfc3339()),
            Box::new(self.updated_at.to_rfc3339()),
            Box::new(serde_json::to_string(&self.metadata).unwrap_or_default()),
        ]
    }
}

impl ToSqlParams for AuditLog {
    fn to_params(&self) -> Vec<Box<dyn rusqlite::ToSql>> {
        vec![
            Box::new(self.actor.clone()),
            Box::new(self.action.clone()),
            Box::new(self.resource_type.clone()),
            Box::new(self.resource_id.clone()),
            Box::new(serde_json::to_string(&self.details).unwrap_or_default()),
            Box::new(self.created_at.to_rfc3339()),
        ]
    }
}

// ── 从 rusqlite Row 解析实体 ───────────────────────────────────

/// 从 `rusqlite::Row` 解析 `Note`。
///
/// v2 列（file_path 等）在旧库/查询未选时走默认值（`unwrap_or`），保持向后兼容。
pub fn note_from_row(row: &rusqlite::Row) -> Result<Note, rusqlite::Error> {
    let metadata_str: String = row.get("metadata")?;
    let metadata = serde_json::from_str(&metadata_str).unwrap_or_default();

    Ok(Note {
        id: row.get("id")?,
        title: row.get("title")?,
        content: row.get("content")?,
        content_type: row.get("content_type")?,
        workspace_id: row.get("workspace_id")?,
        parent_id: row.get("parent_id")?,
        created_at: parse_datetime(row.get::<_, String>("created_at")?),
        updated_at: parse_datetime(row.get::<_, String>("updated_at")?),
        deleted_at: row
            .get::<_, Option<String>>("deleted_at")?
            .map(parse_datetime),
        version: row.get("version")?,
        loro_doc_id: row.get("loro_doc_id")?,
        metadata,
        // V19 §11 v2 字段: 旧查询未选列时用 row.get 的 rusqlite::Error 兜底默认
        file_path: row
            .get::<_, Option<String>>("file_path")
            .ok()
            .flatten()
            .unwrap_or_default(),
        file_hash: row.get::<_, Option<String>>("file_hash").ok().flatten(),
        lamport_ts: row
            .get::<_, Option<i64>>("lamport_ts")
            .ok()
            .flatten()
            .unwrap_or(0),
        sync_state: row
            .get::<_, Option<String>>("sync_state")
            .ok()
            .flatten()
            .unwrap_or_else(|| "synced".into()),
        encryption: row
            .get::<_, Option<String>>("encryption")
            .ok()
            .flatten()
            .unwrap_or_else(|| "none".into()),
        is_deleted: row
            .get::<_, Option<i64>>("is_deleted")
            .ok()
            .flatten()
            .map(|v| v != 0)
            .unwrap_or(false),
    })
}

/// 从 `rusqlite::Row` 解析 `Task`。
pub fn task_from_row(row: &rusqlite::Row) -> Result<Task, rusqlite::Error> {
    let status_str: String = row.get("status")?;
    let status = match status_str.as_str() {
        "done" => TaskStatus::Done,
        "in_progress" => TaskStatus::InProgress,
        "cancelled" => TaskStatus::Cancelled,
        _ => TaskStatus::Todo,
    };
    let metadata_str: String = row.get("metadata")?;
    let metadata = serde_json::from_str(&metadata_str).unwrap_or_default();

    Ok(Task {
        id: row.get("id")?,
        title: row.get("title")?,
        description: row.get("description")?,
        status,
        priority: row.get("priority")?,
        due_date: row
            .get::<_, Option<String>>("due_date")?
            .map(parse_datetime),
        completed_at: row
            .get::<_, Option<String>>("completed_at")?
            .map(parse_datetime),
        note_id: row.get("note_id")?,
        workspace_id: row.get("workspace_id")?,
        created_at: parse_datetime(row.get::<_, String>("created_at")?),
        updated_at: parse_datetime(row.get::<_, String>("updated_at")?),
        metadata,
    })
}

fn parse_datetime(s: String) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(&s)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_roundtrip() {
        let note = Note::new("n-1", "Hello");
        let json = serde_json::to_string(&note).unwrap();
        let decoded: Note = serde_json::from_str(&json).unwrap();
        assert_eq!(note.id, decoded.id);
        assert_eq!(note.title, decoded.title);
    }

    #[test]
    fn task_status_serde() {
        let t = Task::new("t-1", "Task A");
        assert_eq!(t.status, TaskStatus::Todo);
        let json = serde_json::to_string(&t).unwrap();
        assert!(json.contains("\"todo\""));
    }
}
