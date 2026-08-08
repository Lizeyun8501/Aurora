//! 离线队列 (Offline Queue)
//!
//! 基于 SQLite `sync_queue` 表的离线操作队列。
//!
//! # 特性
//! - 优先级 ([`Priority`]::High / Medium / Low)：高优先级先出队。
//! - 幂等键 (idempotency key)：相同操作不重复入队。
//! - 批量压缩：出队时按批次聚合 ([`OfflineQueue::compress_batch`])，降低传输次数。
//!
//! # 数据模型 (SQLite DDL)
//! ```sql
//! CREATE TABLE IF NOT EXISTS sync_queue (
//!   id TEXT PRIMARY KEY,
//!   idempotency_key TEXT UNIQUE NOT NULL,
//!   doc_id TEXT NOT NULL,
//!   payload BLOB NOT NULL,
//!   priority INTEGER NOT NULL,
//!   created_at INTEGER NOT NULL,
//!   attempts INTEGER DEFAULT 0
//! );
//! ```
//!
//! # 实现说明
//! 本模块以内存 [`std::collections::BinaryHeap`] 模拟 SQLite 表与优先级排序。
//! 真实实现使用 `rusqlite` 持久化，公开 API 保持一致。

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};
use std::sync::Arc;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

/// 队列项优先级。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Priority {
    /// 高优先级 (用户显式触发、当前文档)。
    High = 0,
    /// 中优先级 (普通编辑)。
    Medium = 1,
    /// 低优先级 (后台同步、历史回填)。
    Low = 2,
}

impl Priority {
    /// 数值权重 (越小优先级越高)。
    pub fn weight(&self) -> u8 {
        *self as u8
    }
}

impl PartialOrd for Priority {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Priority {
    fn cmp(&self, other: &Self) -> Ordering {
        // High (0) 应大于 Low (2)，故反转数值比较，使其在 max-heap 中先出队
        (*self as u8).cmp(&(*other as u8)).reverse()
    }
}

/// 队列项。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueItem {
    pub id: String,
    pub idempotency_key: String,
    pub doc_id: String,
    pub payload: Vec<u8>,
    pub priority: Priority,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub attempts: u32,
}

impl QueueItem {
    pub fn new(doc_id: impl Into<String>, payload: Vec<u8>, priority: Priority) -> Self {
        let id = uuid::Uuid::new_v4().to_string();
        Self {
            idempotency_key: format!("idk-{}", id),
            doc_id: doc_id.into(),
            payload,
            priority,
            created_at: chrono::Utc::now(),
            id,
            attempts: 0,
        }
    }

    pub fn with_idempotency_key(mut self, key: impl Into<String>) -> Self {
        self.idempotency_key = key.into();
        self
    }
}

impl PartialEq for QueueItem {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
impl Eq for QueueItem {}

impl PartialOrd for QueueItem {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for QueueItem {
    fn cmp(&self, other: &Self) -> Ordering {
        // 优先级高的在前；优先级相同则早创建的在前 (FIFO)
        match self.priority.cmp(&other.priority) {
            Ordering::Equal => other.created_at.cmp(&self.created_at),
            ord => ord,
        }
    }
}

/// SQLite sync_queue 建表语句 (供真实实现参考)。
pub const SCHEMA: &str = r#"CREATE TABLE IF NOT EXISTS sync_queue (
  id TEXT PRIMARY KEY,
  idempotency_key TEXT UNIQUE NOT NULL,
  doc_id TEXT NOT NULL,
  payload BLOB NOT NULL,
  priority INTEGER NOT NULL,
  created_at INTEGER NOT NULL,
  attempts INTEGER DEFAULT 0
);"#;

/// 离线队列 (内存模拟 SQLite 表)。
///
/// 真实实现使用 rusqlite 持久化；接口与内存版一致。
pub struct OfflineQueue {
    heap: Arc<Mutex<BinaryHeap<QueueItem>>>,
    /// 幂等键去重索引：idempotency_key -> item id。
    idempotency_index: Arc<Mutex<HashMap<String, String>>>,
}

impl OfflineQueue {
    pub fn new() -> Self {
        Self {
            heap: Arc::new(Mutex::new(BinaryHeap::new())),
            idempotency_index: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// 入队；若幂等键已存在则返回已有项 ID (去重)。
    pub fn enqueue(&self, item: QueueItem) -> crate::Result<String> {
        let mut idx = self.idempotency_index.lock();
        if let Some(existing_id) = idx.get(&item.idempotency_key) {
            debug!(
                "enqueue dedup: key={} existing={}",
                item.idempotency_key, existing_id
            );
            return Ok(existing_id.clone());
        }
        let id = item.id.clone();
        idx.insert(item.idempotency_key.clone(), id.clone());
        drop(idx);
        self.heap.lock().push(item);
        info!("enqueue ok: id={}", id);
        Ok(id)
    }

    /// 出队最高优先级项 (FIFO 内部排序)，并递增尝试次数。
    pub fn dequeue(&self) -> Option<QueueItem> {
        let mut item = self.heap.lock().pop()?;
        item.attempts += 1;
        Some(item)
    }

    /// 批量出队 (最多 n 项)。
    pub fn dequeue_batch(&self, n: usize) -> Vec<QueueItem> {
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            match self.dequeue() {
                Some(item) => out.push(item),
                None => break,
            }
        }
        out
    }

    /// 合并压缩批次载荷 (mock zstd identity)。
    ///
    /// 真实实现：`zstd::encode_all(&payloads_concat, 3)`。
    pub fn compress_batch(items: &[QueueItem]) -> Vec<u8> {
        bincode::serialize(items).unwrap_or_default()
    }

    /// 解压批次载荷。
    pub fn decompress_batch(bytes: &[u8]) -> crate::Result<Vec<QueueItem>> {
        bincode::deserialize(bytes).map_err(crate::Error::from)
    }

    /// 当前队列长度。
    pub fn len(&self) -> usize {
        self.heap.lock().len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.heap.lock().is_empty()
    }

    /// 确认某幂等键对应的操作已成功 (清理索引，允许未来相同语义重新入队)。
    pub fn ack(&self, idempotency_key: &str) -> crate::Result<()> {
        self.idempotency_index.lock().remove(idempotency_key);
        Ok(())
    }

    /// 幂等键是否已存在于队列中。
    pub fn contains_key(&self, idempotency_key: &str) -> bool {
        self.idempotency_index.lock().contains_key(idempotency_key)
    }
}

impl Default for OfflineQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_priority_ordering_high_first() {
        assert!(Priority::High > Priority::Medium);
        assert!(Priority::Medium > Priority::Low);
        assert_eq!(Priority::High.weight(), 0);
        assert_eq!(Priority::Low.weight(), 2);
    }

    #[test]
    fn test_enqueue_dequeue_basic() {
        let q = OfflineQueue::new();
        let item = QueueItem::new("doc1", vec![1, 2], Priority::Medium);
        let id = q.enqueue(item).expect("enqueue");
        assert!(!id.is_empty());
        assert_eq!(q.len(), 1);
        let popped = q.dequeue().expect("dequeue");
        assert_eq!(popped.id, id);
        assert_eq!(popped.attempts, 1);
        assert!(q.is_empty());
    }

    #[test]
    fn test_priority_queue_ordering() {
        let q = OfflineQueue::new();
        q.enqueue(QueueItem::new("doc", vec![1], Priority::Low))
            .unwrap();
        q.enqueue(QueueItem::new("doc", vec![2], Priority::High))
            .unwrap();
        q.enqueue(QueueItem::new("doc", vec![3], Priority::Medium))
            .unwrap();
        // 应按 High -> Medium -> Low 出队
        let first = q.dequeue().unwrap();
        assert_eq!(first.priority, Priority::High);
        let second = q.dequeue().unwrap();
        assert_eq!(second.priority, Priority::Medium);
        let third = q.dequeue().unwrap();
        assert_eq!(third.priority, Priority::Low);
    }

    #[test]
    fn test_idempotency_dedup() {
        let q = OfflineQueue::new();
        let item = QueueItem::new("doc1", vec![1], Priority::High).with_idempotency_key("key-1");
        let id1 = q.enqueue(item).expect("enqueue");
        // 相同幂等键再次入队应被去重
        let dup = QueueItem::new("doc1", vec![2], Priority::High).with_idempotency_key("key-1");
        let id2 = q.enqueue(dup).expect("enqueue");
        assert_eq!(id1, id2);
        assert_eq!(q.len(), 1);
        assert!(q.contains_key("key-1"));
    }

    #[test]
    fn test_dequeue_batch() {
        let q = OfflineQueue::new();
        for i in 0..5 {
            q.enqueue(QueueItem::new("doc", vec![i], Priority::Medium))
                .unwrap();
        }
        let batch = q.dequeue_batch(3);
        assert_eq!(batch.len(), 3);
        assert_eq!(q.len(), 2);
    }

    #[test]
    fn test_compress_decompress_batch_roundtrip() {
        let items = vec![
            QueueItem::new("doc1", vec![1, 2], Priority::High),
            QueueItem::new("doc2", vec![3, 4], Priority::Low),
        ];
        let compressed = OfflineQueue::compress_batch(&items);
        assert!(!compressed.is_empty());
        let decompressed = OfflineQueue::decompress_batch(&compressed).expect("decompress");
        assert_eq!(decompressed.len(), 2);
        assert_eq!(decompressed[0].doc_id, "doc1");
        assert_eq!(decompressed[1].doc_id, "doc2");
    }

    #[test]
    fn test_ack_removes_idempotency_key() {
        let q = OfflineQueue::new();
        let item = QueueItem::new("doc", vec![1], Priority::High).with_idempotency_key("k");
        q.enqueue(item).unwrap();
        assert!(q.contains_key("k"));
        q.ack("k").unwrap();
        assert!(!q.contains_key("k"));
    }

    #[test]
    fn test_dequeue_empty_returns_none() {
        let q = OfflineQueue::new();
        assert!(q.is_empty());
        assert!(q.dequeue().is_none());
    }

    #[test]
    fn test_schema_contains_required_columns() {
        assert!(SCHEMA.contains("idempotency_key"));
        assert!(SCHEMA.contains("priority"));
        assert!(SCHEMA.contains("payload"));
        assert!(SCHEMA.contains("sync_queue"));
    }

    #[test]
    fn test_fifo_within_same_priority() {
        let q = OfflineQueue::new();
        // 同优先级，先入的先出
        let early = QueueItem::new("doc", vec![1], Priority::Medium).with_idempotency_key("e");
        let early_id = q.enqueue(early).unwrap();
        // 确保 created_at 有差异
        std::thread::sleep(std::time::Duration::from_millis(2));
        let late = QueueItem::new("doc", vec![2], Priority::Medium).with_idempotency_key("l");
        let late_id = q.enqueue(late).unwrap();
        let first = q.dequeue().unwrap();
        assert_eq!(first.id, early_id);
        let second = q.dequeue().unwrap();
        assert_eq!(second.id, late_id);
    }
}
