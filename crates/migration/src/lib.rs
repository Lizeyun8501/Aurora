//! Aurora Note 数据库迁移管理
//!
//! 对应 V19 §29 Schema 定义，提供：
//! - 初始化建表 SQL (notes, tasks, links, workspaces, event_queue, pending_writes, audit_log, version_snapshots, notes_fts)
//! - 版本化迁移机制
//! - FTS5 全文索引支持检测与降级

use rusqlite::OptionalExtension;
use std::sync::Mutex;
use tracing::{error, info, warn};

/// 当前数据库 Schema 版本。
pub const CURRENT_SCHEMA_VERSION: i64 = 1;

/// 迁移管理器。
pub struct MigrationManager {
    conn: Mutex<rusqlite::Connection>,
}

impl MigrationManager {
    /// 打开数据库并初始化迁移表。
    pub fn new(path: impl AsRef<std::path::Path>) -> Result<Self, MigrationError> {
        let conn =
            rusqlite::Connection::open(path).map_err(|e| MigrationError::Open(e.to_string()))?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS _migrations (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL
            )",
            [],
        )
        .map_err(|e| MigrationError::Exec(e.to_string()))?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// 内存中创建（用于测试）。
    pub fn new_in_memory() -> Result<Self, MigrationError> {
        let conn = rusqlite::Connection::open_in_memory()
            .map_err(|e| MigrationError::Open(e.to_string()))?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS _migrations (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL
            )",
            [],
        )
        .map_err(|e| MigrationError::Exec(e.to_string()))?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// 运行所有待执行迁移到最新版本。
    pub fn migrate(&self) -> Result<(), MigrationError> {
        let mut conn = self.conn.lock().unwrap();
        let current = Self::current_version(&conn)?;
        info!(
            current,
            target = CURRENT_SCHEMA_VERSION,
            "starting migration"
        );

        if current < 1 {
            Self::apply_v1(&mut conn)?;
        }

        // 未来版本依次添加：
        // if current < 2 { Self::apply_v2(&mut conn)?; }

        info!(version = CURRENT_SCHEMA_VERSION, "migration completed");
        Ok(())
    }

    fn current_version(conn: &rusqlite::Connection) -> Result<i64, MigrationError> {
        // Create _migrations table if it doesn't exist yet
        conn.execute(
            "CREATE TABLE IF NOT EXISTS _migrations (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL
            )",
            [],
        )
        .map_err(|e| MigrationError::Exec(e.to_string()))?;

        let mut stmt = conn
            .prepare("SELECT MAX(version) FROM _migrations")
            .map_err(|e| MigrationError::Query(e.to_string()))?;
        let version: Option<i64> = stmt
            .query_row([], |row| row.get::<_, Option<i64>>(0))
            .optional()
            .map_err(|e| MigrationError::Query(e.to_string()))?
            .flatten();
        Ok(version.unwrap_or(0))
    }

    fn record_version(conn: &rusqlite::Connection, version: i64) -> Result<(), MigrationError> {
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO _migrations (version, applied_at) VALUES (?1, ?2)",
            rusqlite::params![version, now],
        )
        .map_err(|e| MigrationError::Exec(e.to_string()))?;
        Ok(())
    }

    // ── V1 初始 Schema ──────────────────────────────────────────

    fn apply_v1(conn: &mut rusqlite::Connection) -> Result<(), MigrationError> {
        let tx = conn
            .transaction()
            .map_err(|e| MigrationError::Exec(e.to_string()))?;

        // notes 表
        tx.execute(
            "CREATE TABLE IF NOT EXISTS notes (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                content TEXT NOT NULL DEFAULT '',
                content_type TEXT NOT NULL DEFAULT 'markdown',
                workspace_id TEXT,
                parent_id TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                deleted_at TEXT,
                version INTEGER NOT NULL DEFAULT 1,
                loro_doc_id TEXT,
                metadata TEXT NOT NULL DEFAULT '{}'
            )",
            [],
        )
        .map_err(|e| MigrationError::Exec(e.to_string()))?;

        tx.execute(
            "CREATE INDEX IF NOT EXISTS idx_notes_workspace ON notes(workspace_id)",
            [],
        )
        .map_err(|e| MigrationError::Exec(e.to_string()))?;
        tx.execute(
            "CREATE INDEX IF NOT EXISTS idx_notes_parent ON notes(parent_id)",
            [],
        )
        .map_err(|e| MigrationError::Exec(e.to_string()))?;
        tx.execute(
            "CREATE INDEX IF NOT EXISTS idx_notes_deleted ON notes(deleted_at)",
            [],
        )
        .map_err(|e| MigrationError::Exec(e.to_string()))?;

        // tasks 表
        tx.execute(
            "CREATE TABLE IF NOT EXISTS tasks (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                description TEXT,
                status TEXT NOT NULL DEFAULT 'todo',
                priority INTEGER NOT NULL DEFAULT 0,
                due_date TEXT,
                completed_at TEXT,
                note_id TEXT,
                workspace_id TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                metadata TEXT NOT NULL DEFAULT '{}'
            )",
            [],
        )
        .map_err(|e| MigrationError::Exec(e.to_string()))?;

        tx.execute(
            "CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status)",
            [],
        )
        .map_err(|e| MigrationError::Exec(e.to_string()))?;
        tx.execute(
            "CREATE INDEX IF NOT EXISTS idx_tasks_note ON tasks(note_id)",
            [],
        )
        .map_err(|e| MigrationError::Exec(e.to_string()))?;

        // links 表
        tx.execute(
            "CREATE TABLE IF NOT EXISTS links (
                id TEXT PRIMARY KEY,
                source_note_id TEXT NOT NULL,
                target_note_id TEXT NOT NULL,
                link_type TEXT NOT NULL DEFAULT 'reference',
                created_at TEXT NOT NULL,
                metadata TEXT NOT NULL DEFAULT '{}'
            )",
            [],
        )
        .map_err(|e| MigrationError::Exec(e.to_string()))?;

        tx.execute(
            "CREATE INDEX IF NOT EXISTS idx_links_source ON links(source_note_id)",
            [],
        )
        .map_err(|e| MigrationError::Exec(e.to_string()))?;
        tx.execute(
            "CREATE INDEX IF NOT EXISTS idx_links_target ON links(target_note_id)",
            [],
        )
        .map_err(|e| MigrationError::Exec(e.to_string()))?;

        // workspaces 表
        tx.execute(
            "CREATE TABLE IF NOT EXISTS workspaces (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                metadata TEXT NOT NULL DEFAULT '{}'
            )",
            [],
        )
        .map_err(|e| MigrationError::Exec(e.to_string()))?;

        // event_queue 表
        tx.execute(
            "CREATE TABLE IF NOT EXISTS event_queue (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                channel TEXT NOT NULL,
                event_type TEXT NOT NULL,
                payload TEXT NOT NULL,
                seq INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                consumed_at TEXT
            )",
            [],
        )
        .map_err(|e| MigrationError::Exec(e.to_string()))?;

        tx.execute(
            "CREATE INDEX IF NOT EXISTS idx_event_channel ON event_queue(channel, consumed_at)",
            [],
        )
        .map_err(|e| MigrationError::Exec(e.to_string()))?;
        tx.execute(
            "CREATE INDEX IF NOT EXISTS idx_event_seq ON event_queue(seq)",
            [],
        )
        .map_err(|e| MigrationError::Exec(e.to_string()))?;

        // pending_writes 表 (写前日志)
        tx.execute(
            "CREATE TABLE IF NOT EXISTS pending_writes (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                file_path TEXT NOT NULL,
                tmp_path TEXT NOT NULL,
                loro_op_id TEXT NOT NULL,
                checksum TEXT,
                created_at TEXT NOT NULL
            )",
            [],
        )
        .map_err(|e| MigrationError::Exec(e.to_string()))?;

        tx.execute(
            "CREATE INDEX IF NOT EXISTS idx_pending_op ON pending_writes(loro_op_id)",
            [],
        )
        .map_err(|e| MigrationError::Exec(e.to_string()))?;

        // audit_log 表
        tx.execute(
            "CREATE TABLE IF NOT EXISTS audit_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                actor TEXT NOT NULL,
                action TEXT NOT NULL,
                resource_type TEXT NOT NULL,
                resource_id TEXT NOT NULL,
                details TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL
            )",
            [],
        )
        .map_err(|e| MigrationError::Exec(e.to_string()))?;

        tx.execute(
            "CREATE INDEX IF NOT EXISTS idx_audit_resource ON audit_log(resource_type, resource_id)",
            [],
        )
        .map_err(|e| MigrationError::Exec(e.to_string()))?;
        tx.execute(
            "CREATE INDEX IF NOT EXISTS idx_audit_created ON audit_log(created_at)",
            [],
        )
        .map_err(|e| MigrationError::Exec(e.to_string()))?;

        // version_snapshots 表
        tx.execute(
            "CREATE TABLE IF NOT EXISTS version_snapshots (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                note_id TEXT NOT NULL,
                snapshot_data BLOB NOT NULL,
                version INTEGER NOT NULL,
                created_at TEXT NOT NULL
            )",
            [],
        )
        .map_err(|e| MigrationError::Exec(e.to_string()))?;

        tx.execute(
            "CREATE INDEX IF NOT EXISTS idx_snapshots_note ON version_snapshots(note_id, version)",
            [],
        )
        .map_err(|e| MigrationError::Exec(e.to_string()))?;

        // FTS5 全文索引 (带降级到 FTS4)
        let fts_sql = Self::fts5_create_sql();
        match tx.execute(&fts_sql, []) {
            Ok(_) => info!("FTS index created successfully"),
            Err(e) => {
                warn!(error = %e, "FTS5 creation failed, trying FTS4 fallback");
                tx.execute(
                    "CREATE VIRTUAL TABLE IF NOT EXISTS notes_fts USING fts4(
                        content='notes',
                        title,
                        content
                    )",
                    [],
                )
                .map_err(|e2| {
                    error!(error = %e2, "FTS4 fallback also failed");
                    MigrationError::Exec(format!("FTS init failed: {} / {}", e, e2))
                })?;
            }
        }

        // FTS triggers
        tx.execute(
            "CREATE TRIGGER IF NOT EXISTS notes_fts_insert AFTER INSERT ON notes BEGIN
                INSERT INTO notes_fts(rowid, title, content) VALUES (new.rowid, new.title, new.content);
            END",
            [],
        )
        .map_err(|e| MigrationError::Exec(e.to_string()))?;

        tx.execute(
            "CREATE TRIGGER IF NOT EXISTS notes_fts_update AFTER UPDATE ON notes BEGIN
                UPDATE notes_fts SET title = new.title, content = new.content WHERE rowid = new.rowid;
            END",
            [],
        )
        .map_err(|e| MigrationError::Exec(e.to_string()))?;

        tx.execute(
            "CREATE TRIGGER IF NOT EXISTS notes_fts_delete AFTER DELETE ON notes BEGIN
                DELETE FROM notes_fts WHERE rowid = old.rowid;
            END",
            [],
        )
        .map_err(|e| MigrationError::Exec(e.to_string()))?;

        Self::record_version(&tx, 1)?;
        tx.commit()
            .map_err(|e| MigrationError::Exec(e.to_string()))?;
        info!("applied migration v1");
        Ok(())
    }

    fn fts5_create_sql() -> String {
        "CREATE VIRTUAL TABLE IF NOT EXISTS notes_fts USING fts5(
            content='notes',
            title,
            content,
            tokenize='porter unicode61'
        )"
        .to_string()
    }

    /// 获取底层连接（用于应用层复用同一数据库）。
    pub fn into_inner(self) -> rusqlite::Connection {
        self.conn.into_inner().unwrap()
    }
}

// ── 错误类型 ──────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum MigrationError {
    #[error("database open failed: {0}")]
    Open(String),
    #[error("query failed: {0}")]
    Query(String),
    #[error("execution failed: {0}")]
    Exec(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v1_migration_runs() {
        let mgr = MigrationManager::new_in_memory().unwrap();
        mgr.migrate().unwrap();
        let conn = mgr.into_inner();
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table'")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        assert!(tables.contains(&"notes".to_string()));
        assert!(tables.contains(&"tasks".to_string()));
        assert!(tables.contains(&"links".to_string()));
        assert!(tables.contains(&"workspaces".to_string()));
        assert!(tables.contains(&"event_queue".to_string()));
        assert!(tables.contains(&"pending_writes".to_string()));
        assert!(tables.contains(&"audit_log".to_string()));
        assert!(tables.contains(&"version_snapshots".to_string()));
        assert!(tables.contains(&"notes_fts".to_string()));
    }
}
