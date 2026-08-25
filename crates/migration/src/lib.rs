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
pub const CURRENT_SCHEMA_VERSION: i64 = 2;

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
        let mut conn = self
            .conn
            .lock()
            .map_err(|e| MigrationError::Exec(format!("migration mutex poisoned: {}", e)))?;
        let current = Self::current_version(&conn)?;
        info!(
            current,
            target = CURRENT_SCHEMA_VERSION,
            "starting migration"
        );

        if current < 1 {
            Self::apply_v1(&mut conn)?;
        }
        // V2: notes 表对齐 V19 §11 设计字段（DEV-003）
        if current < 2 {
            Self::apply_v2(&mut conn)?;
        }

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
    pub fn into_inner(self) -> Result<rusqlite::Connection, MigrationError> {
        self.conn
            .into_inner()
            .map_err(|e| MigrationError::Exec(format!("migration mutex poisoned: {}", e)))
    }

    // ── V2: notes 表对齐 V19 §11 设计字段（DEV-003） ────────────

    /// 补齐 V19 设计中 notes 表缺失的 6 个列 + 复合索引：
    /// - `file_path` / `file_hash`: 内容文件系统路径与 SHA-256 校验和（外部同步）
    /// - `lamport_ts`: Lamport 时间戳（外部同步冲突解决）
    /// - `sync_state`: syncing | synced | conflict
    /// - `encryption`: none | shared | private（工作区分级加密）
    /// - `is_deleted`: 与既有 `deleted_at` 并存（V19 命名，软删除标志）
    ///
    /// 兼容性: 全部带默认值，既有行自动回填；`is_deleted` 从 `deleted_at` 回推。
    fn apply_v2(conn: &mut rusqlite::Connection) -> Result<(), MigrationError> {
        let tx = conn
            .transaction()
            .map_err(|e| MigrationError::Exec(e.to_string()))?;

        let ddl: &[&str] = &[
            // 1. file_path: 既有行走 content 内联的旧模式，置空表示未落盘
            "ALTER TABLE notes ADD COLUMN file_path TEXT NOT NULL DEFAULT ''",
            // 2. file_hash: SHA-256 校验和（外部同步完整性）
            "ALTER TABLE notes ADD COLUMN file_hash TEXT",
            // 3. lamport_ts: Lamport 时间戳（外部同步冲突解决，V19 §11）
            "ALTER TABLE notes ADD COLUMN lamport_ts INTEGER NOT NULL DEFAULT 0",
            // 4. sync_state: syncing | synced | conflict（V19 §11）
            "ALTER TABLE notes ADD COLUMN sync_state TEXT NOT NULL DEFAULT 'synced'",
            // 5. encryption: none | shared | private（V19 §11 工作区分级）
            "ALTER TABLE notes ADD COLUMN encryption TEXT NOT NULL DEFAULT 'none'",
            // 6. is_deleted: V19 命名的软删除标志
            "ALTER TABLE notes ADD COLUMN is_deleted INTEGER NOT NULL DEFAULT 0",
        ];
        for sql in ddl {
            tx.execute(sql, [])
                .map_err(|e| MigrationError::Exec(format!("v2 notes: {e}")))?;
        }

        // 回填: deleted_at 非空的行 → is_deleted = 1
        tx.execute(
            "UPDATE notes SET is_deleted = 1 WHERE deleted_at IS NOT NULL",
            [],
        )
        .map_err(|e| MigrationError::Exec(e.to_string()))?;

        // V19 §11 索引: 复合索引（workspace + 最近更新）
        // v1 的旧单列索引同名（idx_notes_workspace），先删再建复合版
        tx.execute("DROP INDEX IF EXISTS idx_notes_workspace", [])
            .map_err(|e| MigrationError::Exec(e.to_string()))?;
        tx.execute(
            "CREATE INDEX IF NOT EXISTS idx_notes_workspace ON notes(workspace_id, updated_at DESC)",
            [],
        )
        .map_err(|e| MigrationError::Exec(e.to_string()))?;
        tx.execute(
            "CREATE INDEX IF NOT EXISTS idx_notes_updated ON notes(updated_at DESC)",
            [],
        )
        .map_err(|e| MigrationError::Exec(e.to_string()))?;
        // sync_state 部分索引: 冲突检测查询
        tx.execute(
            "CREATE INDEX IF NOT EXISTS idx_notes_sync_state ON notes(sync_state) WHERE sync_state != 'synced'",
            [],
        )
        .map_err(|e| MigrationError::Exec(e.to_string()))?;

        tx.commit()
            .map_err(|e| MigrationError::Exec(e.to_string()))?;
        Self::record_version(conn, 2)
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

    #[cfg(test)]
    impl MigrationManager {
        /// 测试专用: 在可变连接上执行 v1（跳过 v2），用于构造旧版本库。
        pub(crate) fn apply_v1_on(conn: &mut rusqlite::Connection) -> Result<(), MigrationError> {
            Self::apply_v1(conn)
        }
    }

    #[cfg(test)]
    #[path = "tests_v2.rs"]
    mod tests_v2;

    #[test]
    fn v1_migration_runs() {
        let mgr = MigrationManager::new_in_memory().unwrap();
        mgr.migrate().unwrap();
        let conn = mgr.into_inner().unwrap();
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
