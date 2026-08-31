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
                consumed_at TEXT
            )",
            [],
        )
        .map_err(|e| crate::Error::Database(format!("sqlite queue create table failed: {}", e)))?;
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
        let created_at = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO event_queue (channel, event_type, payload, seq, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                Self::channel_to_str(&record.channel),
                &record.event_type,
                &record.payload,
                record.seq as i64,
                created_at,
            ],
        )
        .map_err(|e| crate::Error::Database(format!("sqlite queue enqueue failed: {}", e)))?;
        debug!(seq = record.seq, "event persisted to sqlite queue");
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
                Ok(QueuedEvent {
                    seq: seq as u64,
                    channel: Self::str_to_channel(&channel_str),
                    event_type,
                    payload,
                })
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
                Ok(QueuedEvent {
                    seq: seq as u64,
                    channel: Self::str_to_channel(&channel_str),
                    event_type,
                    payload,
                })
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
            seq,
            channel: EventChannel::Medium,
            event_type: "NoteCreated".into(),
            payload: r#"{"id":"n-1"}"#.into(),
        }
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
