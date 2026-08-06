//! 快照 (Snapshot)
//!
//! 周期性地将聚合状态序列化为快照，避免启动时回放全部事件。
//! 默认每 1000 个事件生成一个快照。

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

/// 快照，聚合在某一版本号的不可变状态切面。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    /// 快照唯一 ID
    pub snapshot_id: String,
    /// 所属聚合 ID
    pub aggregate_id: String,
    /// 快照对应的版本号
    pub version: u64,
    /// 序列化后的聚合状态
    pub state: serde_json::Value,
    /// 快照创建时间戳 (毫秒)
    pub created_at: u64,
}

/// 快照管理器，负责判断快照时机与创建快照。
///
/// 目前 `load_latest` 使用进程内缓存，实际的持久化由
/// [`crate::l2_engines::event_sourcing::store::EventStore`] 基于 SQLite 完成。
pub struct SnapshotManager {
    /// 快照间隔，默认 1000
    pub snapshot_interval: usize,
    cache: Mutex<HashMap<String, Snapshot>>,
}

impl Default for SnapshotManager {
    fn default() -> Self {
        Self {
            snapshot_interval: 1000,
            cache: Mutex::new(HashMap::new()),
        }
    }
}

impl SnapshotManager {
    /// 创建一个使用默认快照间隔 (1000) 的快照管理器。
    pub fn new() -> Self {
        Self::default()
    }

    /// 根据当前事件数量判断是否应当生成快照。
    pub fn should_snapshot(&self, event_count: usize) -> bool {
        event_count > 0 && event_count.is_multiple_of(self.snapshot_interval)
    }

    /// 创建一个快照，并将其缓存为该聚合的最新快照。
    pub fn create_snapshot(
        &self,
        aggregate_id: &str,
        version: u64,
        state: serde_json::Value,
    ) -> Snapshot {
        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let snapshot = Snapshot {
            snapshot_id: uuid::Uuid::new_v4().to_string(),
            aggregate_id: aggregate_id.to_string(),
            version,
            state,
            created_at,
        };
        self.cache
            .lock()
            .insert(aggregate_id.to_string(), snapshot.clone());
        snapshot
    }

    /// 从进程内缓存加载指定聚合的最新快照。
    pub fn load_latest(&self, aggregate_id: &str) -> Option<Snapshot> {
        self.cache.lock().get(aggregate_id).cloned()
    }
}
