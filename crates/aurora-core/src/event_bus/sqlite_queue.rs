//! EventQueueStore SQLite 生产实现
//!
//! 对应 V19 ARCH-003 崩溃恢复要求：事件持久化到 SQLite `event_queue` 表。
//! 启动时重放未消费事件，保证跨进程/跨会话的事件不丢失。

use std::sync::Mutex;

use chrono::Utc;
use tracing::debug;

use crate::event_bus::layered::{EventQueueStore, QueuedEvent};

/// 基于 SQLite 的事件队列存储。
pub struct SqliteEventQueue {
    conn: Mutex<rusqlite::Connection>,
}

impl SqliteEventQueue {
    /// 打开 SQLite 数据库并初始化事件队列表。
    ///
    /// 要求目标数据库已包含 `event_queue` 表（由 `aurora-migration` 初始化）。
    pub fn new(path: impl AsRef<std::path::Path>) -> Result<Self, crate::Error> {
        let conn = rusqlite::Connection::open(path)
            .map_err(|e| crate::Error::Database(format!("sqlite queue open failed: {}", e)))?;
        Self::ensure_schema(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// 使用已有连接（共享同一数据库）。
    pub fn from_connection(conn: rusqlite::Connection) -> Self {
        Self {
            conn: Mutex::new(conn),
        }
    }

    /// 内存中创建（用于测试）。
    pub fn new_in_memory() -> Result<Self, crate::Error> {
        let conn = rusqlite::Connection::open_in_memory().map_err(|e| {
            crate::Error::Database(format!("sqlite queue in-memory open failed: {}", e))
        })?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS event_queue (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                channel TEXT NOT NULL,
                event_type TEXT NOT NULL,
                payload TEXT NOT NULL,
                seq INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                consumed_at TEXT,
                event_id TEXT
            )",
            [],
        )
        .map_err(|e| crate::Error::Database(format!("sqlite queue create table failed: {}", e)))?;
        Self::ensure_schema(&conn)?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_event_channel ON event_queue(channel, consumed_at)",
            [],
        )
        .map_err(|e| crate::Error::Database(format!("sqlite queue create index failed: {}", e)))?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn channel_to_str(ch: &crate::event_bus::layered::EventChannel) -> &'static str {
        match ch {
            crate::event_bus::layered::EventChannel::High => "high",
            crate::event_bus::layered::EventChannel::Medium => "medium",
            crate::event_bus::layered::EventChannel::Low => "low",
        }
    }

    /// V23-I0（T2/T11）: 旧库列迁移 + 去重索引 + 水位线表。
    /// 五步法「新增先行」— 不改已有列，仅 ADD COLUMN（幂等）。
    fn ensure_schema(conn: &rusqlite::Connection) -> Result<(), crate::Error> {
        // 幂等建表（全新库独立可用; 与 aurora-migration 的建表语句一致）
        conn.execute(
            "CREATE TABLE IF NOT EXISTS event_queue (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                channel TEXT NOT NULL,
                event_type TEXT NOT NULL,
                payload TEXT NOT NULL,
                seq INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                consumed_at TEXT,
                event_id TEXT
            )",
            [],
        )
        .map_err(|e| crate::Error::Database(format!("event_queue create: {e}")))?;
        let has_event_id: bool = {
            let mut stmt = conn
                .prepare("PRAGMA table_info(event_queue)")
                .map_err(|e| crate::Error::Database(format!("table_info: {e}")))?;
            let cols: Vec<String> = stmt
                .query_map([], |r| r.get::<_, String>(1))
                .map_err(|e| crate::Error::Database(format!("table_info cols: {e}")))?
                .filter_map(|r| r.ok())
                .collect();
            cols.iter().any(|c| c == "event_id")
        };
        if !has_event_id {
            conn.execute("ALTER TABLE event_queue ADD COLUMN event_id TEXT", [])
                .map_err(|e| crate::Error::Database(format!("add event_id: {e}")))?;
        }
        conn.execute(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_event_dedup
             ON event_queue(event_id) WHERE event_id IS NOT NULL",
            [],
        )
        .map_err(|e| crate::Error::Database(format!("dedup index: {e}")))?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS projection_watermark (
                projection TEXT PRIMARY KEY,
                watermark INTEGER NOT NULL,
                updated_at TEXT NOT NULL
            )",
            [],
        )
        .map_err(|e| crate::Error::Database(format!("watermark table: {e}")))?;
        Ok(())
    }

    fn enqueue_idempotent(&self, record: &QueuedEvent) -> Result<bool, crate::Error> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| crate::Error::Internal("sqlite queue mutex poisoned".into()))?;
        let inserted = Self::do_insert(&conn, record)?;
        Ok(inserted > 0)
    }

    fn watermark(&self, projection: &str) -> Result<u64, crate::Error> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| crate::Error::Internal("sqlite queue mutex poisoned".into()))?;
        let v: Option<i64> = conn
            .query_row(
                "SELECT watermark FROM projection_watermark WHERE projection = ?1",
                [projection],
                |r| r.get(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(crate::Error::Database(format!("watermark get: {other}"))),
            })?;
        Ok(v.map(|x| x as u64).unwrap_or(0))
    }

    fn set_watermark(&self, projection: &str, seq: u64) -> Result<(), crate::Error> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| crate::Error::Internal("sqlite queue mutex poisoned".into()))?;
        conn.execute(
            "INSERT INTO projection_watermark (projection, watermark, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(projection) DO UPDATE SET watermark = ?2, updated_at = ?3",
            rusqlite::params![projection, seq as i64, Utc::now().to_rfc3339()],
        )
        .map_err(|e| crate::Error::Database(format!("watermark set: {e}")))?;
        Ok(())
    }

    /// 实际插入（返回影响行数: 0 = 幂等忽略）。
    fn do_insert(conn: &rusqlite::Connection, record: &QueuedEvent) -> Result<usize, crate::Error> {
        let created_at = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT OR IGNORE INTO event_queue (channel, event_type, payload, seq, created_at, event_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                Self::channel_to_str(&record.channel),
                &record.event_type,
                &record.payload,
                record.seq as i64,
                created_at,
                record.event_id,
            ],
        )
        .map_err(|e| crate::Error::Database(format!("sqlite queue enqueue failed: {}", e)))
    }

    fn str_to_channel(s: &str) -> crate::event_bus::layered::EventChannel {
        match s {
            "high" => crate::event_bus::layered::EventChannel::High,
            "medium" => crate::event_bus::layered::EventChannel::Medium,
            _ => crate::event_bus::layered::EventChannel::Low,
        }
    }
}

impl EventQueueStore for SqliteEventQueue {
    fn enqueue(&self, record: &QueuedEvent) -> Result<(), crate::Error> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| crate::Error::Internal("sqlite queue mutex poisoned".into()))?;
        let inserted = Self::do_insert(&conn, record)?;
        if inserted == 0 {
            debug!(seq = record.seq, event_id = ?record.event_id, "duplicate event ignored (idempotent)");
        } else {
            debug!(seq = record.seq, "event persisted to sqlite queue");
        }
        Ok(())
    }

    fn mark_consumed(&self, seq: u64) -> Result<(), crate::Error> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| crate::Error::Internal("sqlite queue mutex poisoned".into()))?;
        let consumed_at = Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE event_queue SET consumed_at = ?1 WHERE seq = ?2",
            rusqlite::params![consumed_at, seq as i64],
        )
        .map_err(|e| crate::Error::Database(format!("sqlite queue mark_consumed failed: {}", e)))?;
        debug!(seq, "event marked consumed in sqlite queue");
        Ok(())
    }

    /// 投影 catch_up 重放：读取 seq > from 的全部事件（含已消费）。
    fn events_after(&self, from: u64) -> Result<Vec<QueuedEvent>, crate::Error> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| crate::Error::Internal("sqlite queue mutex poisoned".into()))?;
        let mut stmt = conn
            .prepare("SELECT seq, channel, event_type, payload FROM event_queue WHERE seq > ?1 ORDER BY seq ASC")
            .map_err(|e| crate::Error::Database(format!("events_after prepare: {}", e)))?;
        let rows = stmt
            .query_map([from as i64], |row| {
                let seq: i64 = row.get(0)?;
                let channel_str: String = row.get(1)?;
                let event_type: String = row.get(2)?;
                let payload: String = row.get(3)?;
                Ok(QueuedEvent::legacy(seq as u64, Self::str_to_channel(&channel_str), event_type, payload))
            })
            .map_err(|e| crate::Error::Database(format!("events_after query: {}", e)))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| {
                crate::Error::Database(format!("events_after row parse failed: {}", e))
            })?);
        }
        Ok(out)
    }

    fn pending(&self) -> Result<Vec<QueuedEvent>, crate::Error> {
        // 仅 Medium 通道未消费事件（Medium 重放语义 §32.2）；
        // Low 事件虽持久化（投影追赶用）但不参与此语义。
        let conn = self
            .conn
            .lock()
            .map_err(|_| crate::Error::Internal("sqlite queue mutex poisoned".into()))?;
        let mut stmt = conn
            .prepare("SELECT seq, channel, event_type, payload FROM event_queue WHERE consumed_at IS NULL AND channel = 'medium' ORDER BY seq")
            .map_err(|e| crate::Error::Database(format!("sqlite queue prepare failed: {}", e)))?;
        let rows = stmt
            .query_map([], |row| {
                let seq: i64 = row.get(0)?;
                let channel_str: String = row.get(1)?;
                let event_type: String = row.get(2)?;
                let payload: String = row.get(3)?;
                Ok(QueuedEvent::legacy(seq as u64, Self::str_to_channel(&channel_str), event_type, payload))
            })
            .map_err(|e| crate::Error::Database(format!("sqlite queue query failed: {}", e)))?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| {
                crate::Error::Database(format!("sqlite queue row parse failed: {}", e))
            })?);
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_bus::layered::{EventChannel, EventQueueStore, QueuedEvent};

    fn make_event(seq: u64) -> QueuedEvent {
        QueuedEvent {
            event_id: None,
            seq,
            channel: EventChannel::Medium,
            event_type: "NoteCreated".into(),
            payload: r#"{"id":"n-1"}"#.into(),
        }
    }

    /// T2（V23-I0）: 同 event_id 二次入队零副作用。
    #[test]
    fn t2_idempotent_enqueue_dedups() {
        let queue = SqliteEventQueue::new_in_memory().unwrap();
        let mut ev = make_event(1);
        ev.event_id = Some("evt-abc".into());

        assert!(queue.enqueue_idempotent(&ev).unwrap(), "first insert");
        assert!(!queue.enqueue_idempotent(&ev).unwrap(), "dup ignored");
        // 重放场景: 不同 seq 同 event_id 也被拒（键在 event_id）
        let mut ev2 = make_event(99);
        ev2.event_id = Some("evt-abc".into());
        assert!(!queue.enqueue_idempotent(&ev2).unwrap(), "dup by event_id");

        assert_eq!(queue.pending().unwrap().len(), 1, "only one physical row");
    }

    /// T2: 无 event_id 不去重（兼容旧路径）。
    #[test]
    fn t2_legacy_events_not_deduped() {
        let queue = SqliteEventQueue::new_in_memory().unwrap();
        queue.enqueue(&make_event(1)).unwrap();
        queue.enqueue(&make_event(2)).unwrap();
        assert_eq!(queue.pending().unwrap().len(), 2);
    }

    /// T11（V23-I0）: 水位线持久化 — 崩溃恢复不重放不遗漏。
    #[test]
    fn t11_watermark_survives_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("q.db");
        {
            let queue = SqliteEventQueue::new(&db).unwrap();
            queue.set_watermark("bidi_link", 42).unwrap();
            queue.set_watermark("task_projection", 7).unwrap();
        }
        // 模拟进程重启: 重新打开同一库
        let queue = SqliteEventQueue::new(&db).unwrap();
        assert_eq!(queue.watermark("bidi_link").unwrap(), 42);
        assert_eq!(queue.watermark("task_projection").unwrap(), 7);
        // 未设置过的投影 → 0（从头重放）
        assert_eq!(queue.watermark("unknown").unwrap(), 0);
        // 更新语义: 前进
        queue.set_watermark("bidi_link", 100).unwrap();
        assert_eq!(queue.watermark("bidi_link").unwrap(), 100);
    }

    #[test]
    fn sqlite_enqueue_and_pending() {
        let queue = SqliteEventQueue::new_in_memory().unwrap();
        queue.enqueue(&make_event(1)).unwrap();
        queue.enqueue(&make_event(2)).unwrap();
        let pending = queue.pending().unwrap();
        assert_eq!(pending.len(), 2);
        assert_eq!(pending[0].seq, 1);
        assert_eq!(pending[1].seq, 2);
    }

    #[test]
    fn sqlite_mark_consumed() {
        let queue = SqliteEventQueue::new_in_memory().unwrap();
        queue.enqueue(&make_event(42)).unwrap();
        queue.mark_consumed(42).unwrap();
        let pending = queue.pending().unwrap();
        assert!(pending.is_empty());
    }

    /// V20 Phase 1 语义: pending() 只返回 Medium 未消费；
    /// Low 事件经 events_after() 供投影追赶（含已消费）。
    #[test]
    fn sqlite_channel_roundtrip() {
        let queue = SqliteEventQueue::new_in_memory().unwrap();
        let mut low = make_event(1);
        low.channel = EventChannel::Low;
        queue.enqueue(&low).unwrap();
        let mut med = make_event(2);
        med.channel = EventChannel::Medium;
        queue.enqueue(&med).unwrap();

        // pending: 仅 Medium
        let pending = queue.pending().unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].seq, 2);
        assert_eq!(pending[0].channel, EventChannel::Medium);

        // events_after: 全部（投影追赶，含 Low）
        let all = queue.events_after(0).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].channel, EventChannel::Low);
        assert_eq!(all[1].channel, EventChannel::Medium);

        // 已消费后 events_after 仍可见（水位线语义），pending 不见
        queue.mark_consumed(2).unwrap();
        assert!(queue.pending().unwrap().is_empty());
        assert_eq!(queue.events_after(0).unwrap().len(), 2);
    }
}
