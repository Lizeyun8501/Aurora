//! 事件存储 (Event Store)
//!
//! 基于 SQLite (WAL 模式) 持久化事件序列与快照。通过 `parking_lot::Mutex`
//! 保护连接，保证多线程下的安全访问。

use parking_lot::Mutex;
use rusqlite::{params, Connection, OptionalExtension};

use super::event::Event;
use super::snapshot::Snapshot;
use crate::Error;

/// 事件存储，封装 SQLite 连接，提供事件与快照的读写能力。
pub struct EventStore {
    conn: Mutex<Connection>,
}

impl EventStore {
    /// 打开 (或创建) 指定路径的 SQLite 数据库，启用 WAL 模式并初始化表结构。
    pub fn new(db_path: &str) -> Result<Self, Error> {
        let conn = Connection::open(db_path).map_err(|e| Error::Database(e.to_string()))?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
            CREATE TABLE IF NOT EXISTS events (
                event_id TEXT PRIMARY KEY,
                aggregate_id TEXT NOT NULL,
                block_id TEXT,
                op_type TEXT NOT NULL,
                payload TEXT NOT NULL,
                timestamp INTEGER NOT NULL,
                user_id TEXT,
                device_id TEXT,
                signature BLOB
            );
            CREATE INDEX IF NOT EXISTS idx_events_aggregate ON events(aggregate_id, timestamp);
            CREATE TABLE IF NOT EXISTS snapshots (
                snapshot_id TEXT PRIMARY KEY,
                aggregate_id TEXT NOT NULL,
                version INTEGER NOT NULL,
                state TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_snapshots_aggregate ON snapshots(aggregate_id, version DESC);",
        )
        .map_err(|e| Error::Database(e.to_string()))?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// 追加一个事件到存储。`aggregate_id` 取自事件的 `block_id`。
    pub fn append_event(&self, event: &Event) -> Result<(), Error> {
        let payload = serde_json::to_string(&event.payload).map_err(Error::Serialization)?;
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO events (event_id, aggregate_id, block_id, op_type, payload, timestamp, user_id, device_id, signature) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                event.event_id,
                event.block_id,
                event.block_id,
                event.op_type,
                payload,
                event.timestamp as i64,
                event.user_id,
                event.device_id,
                event.signature.as_deref(),
            ],
        )
        .map_err(|e| Error::Database(e.to_string()))?;
        Ok(())
    }

    /// 查询指定聚合的事件，按时间戳升序返回。
    ///
    /// 当 `since` 为 `Some(t)` 时仅返回 `timestamp > t` 的事件，否则返回全部事件。
    pub fn get_events(&self, aggregate_id: &str, since: Option<u64>) -> Result<Vec<Event>, Error> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare(
                "SELECT event_id, block_id, op_type, payload, timestamp, user_id, device_id, signature \
                 FROM events WHERE aggregate_id = ?1 AND (?2 IS NULL OR timestamp > ?2) \
                 ORDER BY timestamp ASC",
            )
            .map_err(|e| Error::Database(e.to_string()))?;
        let since_i64 = since.map(|t| t as i64);
        let events = stmt
            .query_map(params![aggregate_id, since_i64], |row| {
                let payload_str: String = row.get(3)?;
                let payload: serde_json::Value =
                    serde_json::from_str(&payload_str).unwrap_or(serde_json::Value::Null);
                let block_id: Option<String> = row.get(1)?;
                let user_id: Option<String> = row.get(5)?;
                let device_id: Option<String> = row.get(6)?;
                let signature: Option<Vec<u8>> = row.get(7)?;
                Ok(Event {
                    event_id: row.get(0)?,
                    block_id: block_id.unwrap_or_default(),
                    op_type: row.get(2)?,
                    payload,
                    timestamp: row.get::<_, i64>(4)? as u64,
                    user_id: user_id.unwrap_or_default(),
                    device_id: device_id.unwrap_or_default(),
                    signature,
                })
            })
            .map_err(|e| Error::Database(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| Error::Database(e.to_string()))?;
        Ok(events)
    }

    /// 保存一个快照。
    pub fn save_snapshot(&self, snapshot: &Snapshot) -> Result<(), Error> {
        let state = serde_json::to_string(&snapshot.state).map_err(Error::Serialization)?;
        let conn = self.conn.lock();
        conn.execute(
            "INSERT OR REPLACE INTO snapshots (snapshot_id, aggregate_id, version, state, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                snapshot.snapshot_id,
                snapshot.aggregate_id,
                snapshot.version as i64,
                state,
                snapshot.created_at as i64,
            ],
        )
        .map_err(|e| Error::Database(e.to_string()))?;
        Ok(())
    }

    /// 获取指定聚合的最新快照。
    pub fn get_latest_snapshot(&self, aggregate_id: &str) -> Result<Option<Snapshot>, Error> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare(
                "SELECT snapshot_id, aggregate_id, version, state, created_at \
                 FROM snapshots WHERE aggregate_id = ?1 ORDER BY version DESC LIMIT 1",
            )
            .map_err(|e| Error::Database(e.to_string()))?;
        let snapshot = stmt
            .query_row(params![aggregate_id], |row| {
                let state_str: String = row.get(3)?;
                let state: serde_json::Value =
                    serde_json::from_str(&state_str).unwrap_or(serde_json::Value::Null);
                Ok(Snapshot {
                    snapshot_id: row.get(0)?,
                    aggregate_id: row.get(1)?,
                    version: row.get::<_, i64>(2)? as u64,
                    state,
                    created_at: row.get::<_, i64>(4)? as u64,
                })
            })
            .optional()
            .map_err(|e| Error::Database(e.to_string()))?;
        Ok(snapshot)
    }

    /// 加载指定聚合的最新快照及其后的增量事件。
    ///
    /// 若无快照则返回 `(None, 全部事件)`。
    pub fn get_events_since_snapshot(
        &self,
        aggregate_id: &str,
    ) -> Result<(Option<Snapshot>, Vec<Event>), Error> {
        let snapshot = self.get_latest_snapshot(aggregate_id)?;
        let since = snapshot.as_ref().map(|s| s.version);
        let events = self.get_events(aggregate_id, since)?;
        Ok((snapshot, events))
    }
}

#[cfg(test)]
mod tests {
    use super::EventStore;
    use crate::l2_engines::event_sourcing::event::{Event, OpType};
    use crate::l2_engines::event_sourcing::snapshot::Snapshot;

    #[test]
    fn test_create_and_append_event() {
        let store = EventStore::new(":memory:").unwrap();
        let event = Event::new(
            "block-1",
            OpType::Create,
            serde_json::json!({"text": "Hello"}),
            "user-1",
            "device-1",
        );
        store.append_event(&event).unwrap();
        let events = store.get_events("block-1", None).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].block_id, "block-1");
        assert_eq!(events[0].op_type, "create");
        assert_eq!(events[0].user_id, "user-1");
    }

    #[test]
    fn test_get_events_since_timestamp() {
        let store = EventStore::new(":memory:").unwrap();
        let e1 = Event::new("block-1", OpType::Create, serde_json::json!({"v": 1}), "u", "d");
        store.append_event(&e1).unwrap();
        let e2 = Event::new("block-1", OpType::Update, serde_json::json!({"v": 2}), "u", "d");
        store.append_event(&e2).unwrap();

        let all = store.get_events("block-1", None).unwrap();
        assert_eq!(all.len(), 2);

        let since = store.get_events("block-1", Some(e1.timestamp)).unwrap();
        assert_eq!(since.len(), 1);
        assert_eq!(since[0].event_id, e2.event_id);
    }

    #[test]
    fn test_snapshot_save_and_load() {
        let store = EventStore::new(":memory:").unwrap();
        let snapshot = Snapshot {
            snapshot_id: "snap-1".to_string(),
            aggregate_id: "agg-1".to_string(),
            version: 100,
            state: serde_json::json!({"blocks": {}}),
            created_at: 0,
        };
        store.save_snapshot(&snapshot).unwrap();
        let loaded = store.get_latest_snapshot("agg-1").unwrap();
        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.snapshot_id, "snap-1");
        assert_eq!(loaded.version, 100);
    }

    #[test]
    fn test_get_events_since_snapshot() {
        let store = EventStore::new(":memory:").unwrap();
        let e1 = Event::new("agg-1", OpType::Create, serde_json::json!({"v": 1}), "u", "d");
        store.append_event(&e1).unwrap();

        // 快照版本等于 e1.timestamp，增量查询应排除 e1
        let snapshot = Snapshot {
            snapshot_id: "snap-1".to_string(),
            aggregate_id: "agg-1".to_string(),
            version: e1.timestamp,
            state: serde_json::json!({}),
            created_at: 0,
        };
        store.save_snapshot(&snapshot).unwrap();

        let e2 = Event::new("agg-1", OpType::Update, serde_json::json!({"v": 2}), "u", "d");
        store.append_event(&e2).unwrap();

        let (snap, events) = store.get_events_since_snapshot("agg-1").unwrap();
        assert!(snap.is_some());
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_id, e2.event_id);
    }
}
