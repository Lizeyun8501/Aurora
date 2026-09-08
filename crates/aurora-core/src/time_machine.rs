//! 时间机器（V23-I2→I4 / T20 兄弟项 — 事件溯源能力的用户可感知兑现）
//!
//! 「快照点回溯」：每次内容变更（防抖合并后）写入 version_snapshots，
//! 用户可列出版本并一键回溯（回溯即写回 kv + 事件发布 → mirror/blocks
//! 双轨自动跟随，因为走同一保存路径）。
//!
//! 治理（V22.1 §4.10 / R6 存储成本入账）:
//! - 每 note 保留最近 [`MAX_SNAPSHOTS_PER_NOTE`]（20）版，超出删最旧
//! - kv/OpLog 仍是事实源 — 快照损坏不影响一致性（逃生出口）

use rusqlite::Connection;

/// 每笔记保留的快照上限（R6: 存储成本治理）。
pub const MAX_SNAPSHOTS_PER_NOTE: usize = 20;

/// 单个版本元数据（列表用 — 不含数据体）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct SnapshotMeta {
    pub version: i64,
    pub created_at: String,
    pub size: usize,
}

/// 时间机器存储（version_snapshots 表，migration V1 建）。
pub struct TimeMachine {
    conn: std::sync::Mutex<Connection>,
}

impl TimeMachine {
    pub fn new(conn: Connection) -> Self {
        Self { conn: std::sync::Mutex::new(conn) }
    }

    /// 写入快照（version = 该 note 现存最大版本 + 1; 超限裁剪最旧）。
    pub fn save(&self, note_id: &str, data: &[u8]) -> Result<i64, crate::Error> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| crate::Error::Internal("timemachine mutex poisoned".to_string()))?;
        let next: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) + 1 FROM version_snapshots WHERE note_id = ?1",
                [note_id],
                |r| r.get(0),
            )
            .map_err(|e| crate::Error::Database(e.to_string()))?;
        conn.execute(
            "INSERT INTO version_snapshots (note_id, snapshot_data, version, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                note_id,
                data,
                next,
                chrono::Utc::now().to_rfc3339()
            ],
        )
        .map_err(|e| crate::Error::Database(e.to_string()))?;
        // 治理: 超限删最旧
        conn.execute(
            "DELETE FROM version_snapshots WHERE note_id = ?1 AND version <= ?2",
            rusqlite::params![note_id, next as i64 - MAX_SNAPSHOTS_PER_NOTE as i64],
        )
        .map_err(|e| crate::Error::Database(e.to_string()))?;
        Ok(next)
    }

    /// 列出版本（新→旧，不含数据体）。
    pub fn list(&self, note_id: &str) -> Result<Vec<SnapshotMeta>, crate::Error> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| crate::Error::Internal("timemachine mutex poisoned".to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT version, created_at, LENGTH(snapshot_data)
                 FROM version_snapshots WHERE note_id = ?1
                 ORDER BY version DESC",
            )
            .map_err(|e| crate::Error::Database(e.to_string()))?;
        let rows = stmt
            .query_map([note_id], |r| {
                Ok(SnapshotMeta {
                    version: r.get(0)?,
                    created_at: r.get(1)?,
                    size: r.get::<_, i64>(2).unwrap_or(0) as usize,
                })
            })
            .map_err(|e| crate::Error::Database(e.to_string()))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| crate::Error::Database(e.to_string()))
    }

    /// 读取指定版本数据体（回溯 = 调用方将此数据写回 kv 走保存路径）。
    pub fn load(&self, note_id: &str, version: i64) -> Result<Option<Vec<u8>>, crate::Error> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| crate::Error::Internal("timemachine mutex poisoned".to_string()))?;
        let row = conn
            .query_row(
                "SELECT snapshot_data FROM version_snapshots WHERE note_id = ?1 AND version = ?2",
                rusqlite::params![note_id, version],
                |r| r.get::<_, Vec<u8>>(0),
            )
            .optional()
            .map_err(|e| crate::Error::Database(e.to_string()))?;
        Ok(row)
    }
}

use rusqlite::OptionalExtension;

#[cfg(test)]
mod tests {
    use super::*;

    fn tm() -> TimeMachine {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE version_snapshots (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                note_id TEXT NOT NULL, snapshot_data BLOB NOT NULL,
                version INTEGER NOT NULL, created_at TEXT NOT NULL)",
            [],
        )
        .unwrap();
        TimeMachine::new(conn)
    }

    /// 写入递增版本 + 列表新→旧 + load 精确取回。
    #[test]
    fn save_list_load_roundtrip() {
        let tm = tm();
        let v1 = tm.save("n1", b"content-v1").unwrap();
        let v2 = tm.save("n1", b"content-v2").unwrap();
        assert_eq!((v1, v2), (1, 2));
        let list = tm.list("n1").unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].version, 2, "新→旧");
        assert_eq!(list[0].size, 10);
        let data = tm.load("n1", 1).unwrap().unwrap();
        assert_eq!(data, b"content-v1");
        assert!(tm.load("n1", 99).unwrap().is_none());
        assert!(tm.list("other").unwrap().is_empty());
    }

    /// R6 治理: 每 note 上限 20 版，超出删最旧（滑动窗口）。
    #[test]
    fn snapshot_cap_prunes_oldest() {
        let tm = tm();
        for i in 0..25 {
            tm.save("n1", format!("v{i}").as_bytes()).unwrap();
        }
        let list = tm.list("n1").unwrap();
        assert_eq!(list.len(), MAX_SNAPSHOTS_PER_NOTE);
        // 最旧 5 版（v1..=v5）已删 — 可取的最旧版本是 6
        let oldest = tm.load("n1", 5).unwrap();
        assert!(oldest.is_none());
        let kept = tm.load("n1", 6).unwrap().unwrap();
        assert_eq!(kept, b"v5"); // 第 6 次写入的内容是 v5（i 从 0 计）
        // 其他 note 不受影响
        tm.save("n2", b"x").unwrap();
        assert_eq!(tm.list("n2").unwrap().len(), 1);
    }
}
