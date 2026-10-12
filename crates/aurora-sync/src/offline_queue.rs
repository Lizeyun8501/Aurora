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
//! # 实现说明 (DK-51 案一)
//! 已 rusqlite 真实化：[`OfflineQueue::new`] 进程内内存库（行为与旧
//! 内存版同构），[`OfflineQueue::open`] 落盘持久版（WAL + 幂等建表）。
//! 公开 API 与旧内存版完全一致，消费端无感知。

use std::cmp::Ordering;

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

/// 离线队列 (rusqlite 真实化，DK-51 案一)。
///
/// 存储即 [`SCHEMA`] 的 `sync_queue` 表：
/// - `new()` = 进程内 SQLite（内存库）——签名与旧行为完全一致（测试/
///   默认装配零改动）；
/// - [`OfflineQueue::open`] = 落盘版（`data_dir/sync_queue.db`）——
///   进程重启队列不丢，两端装配点（aurora-bootstrap 单点）已切换。
///
/// 语义对齐内存版（消费端无感知）：
/// - 出队即离队（行删除）；补发失败经 [`OfflineQueue::requeue`] 回插，
///   attempts 随 INSERT 落库——进程中断不丢重试计数；
/// - 幂等键 `UNIQUE` 约束由表承担（INSERT OR IGNORE 去重）；
/// - `ack` 清除该键对应行（操作已成功语义；生产流 ack 均在出队后
///   调用，行已离队，行为与内存版等价）。
pub struct OfflineQueue {
    conn: Mutex<rusqlite::Connection>,
}

impl OfflineQueue {
    /// 进程内 SQLite 版（内存库，与旧内存版同构：进程结束即消失）。
    pub fn new() -> Self {
        let conn =
            rusqlite::Connection::open_in_memory().expect("in-memory sqlite always available");
        Self::init_conn(conn)
    }

    /// 落盘版：打开（或创建）持久化队列库。
    ///
    /// WAL 模式（崩溃安全：enqueue 落库即持久）；表结构幂等创建。
    pub fn open(path: &std::path::Path) -> crate::Result<Self> {
        let conn =
            rusqlite::Connection::open(path).map_err(|e| crate::Error::Database(e.to_string()))?;
        // WAL：写不阻塞读 + 断电安全（队列项是少量小行，checkpoint 开销可忽略）
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|e| crate::Error::Database(e.to_string()))?;
        Ok(Self::init_conn(conn))
    }

    /// 幂等建表 + 包 Mutex。
    fn init_conn(conn: rusqlite::Connection) -> Self {
        conn.execute_batch(SCHEMA).expect("sync_queue schema valid");
        Self {
            conn: Mutex::new(conn),
        }
    }

    /// 入队；若幂等键已存在则返回已有项 ID (去重)。
    pub fn enqueue(&self, item: QueueItem) -> crate::Result<String> {
        let conn = self.conn.lock();
        let affected = conn
            .execute(
                "INSERT OR IGNORE INTO sync_queue
                 (id, idempotency_key, doc_id, payload, priority, created_at, attempts)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![
                    item.id,
                    item.idempotency_key,
                    item.doc_id,
                    item.payload,
                    item.priority.weight(),
                    item.created_at.timestamp_micros(),
                    item.attempts,
                ],
            )
            .map_err(|e| crate::Error::Database(e.to_string()))?;
        if affected == 0 {
            // 幂等键 UNIQUE 冲突 → 返回已有项 ID（去重语义与内存版一致）
            let existing: String = conn
                .query_row(
                    "SELECT id FROM sync_queue WHERE idempotency_key = ?1",
                    rusqlite::params![item.idempotency_key],
                    |r| r.get(0),
                )
                .map_err(|e| crate::Error::Database(e.to_string()))?;
            debug!(
                "enqueue dedup: key={} existing={}",
                item.idempotency_key, existing
            );
            return Ok(existing);
        }
        info!("enqueue ok: id={}", item.id);
        Ok(item.id)
    }

    /// 出队最高优先级项 (FIFO 内部排序)，并递增尝试次数。
    ///
    /// 出队即离队（行删除，与内存版堆弹出一致）；attempts 递增随返回值
    /// 携带，由 [`OfflineQueue::requeue`] 落库持久。
    pub fn dequeue(&self) -> Option<QueueItem> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare(
                "SELECT id, idempotency_key, doc_id, payload, priority, created_at, attempts
             FROM sync_queue ORDER BY priority ASC, created_at ASC LIMIT 1",
            )
            .ok()?;
        let item = stmt.query_row([], Self::row_to_item).ok()?;
        drop(stmt);
        // 同连接内删除（出队即离队）
        conn.execute(
            "DELETE FROM sync_queue WHERE id = ?1",
            rusqlite::params![item.id],
        )
        .ok()?;
        let mut item = item;
        item.attempts += 1;
        Some(item)
    }

    /// DK-40b: 补发失败回队——item 回插表中，幂等索引保持不动
    /// （失败≠放弃，语义与 ack 相反；attempts 已由 dequeue 递增，
    /// INSERT 落库 → 进程中断不丢重试计数）。
    pub fn requeue(&self, item: QueueItem) {
        let conn = self.conn.lock();
        let _ = conn.execute(
            "INSERT OR REPLACE INTO sync_queue
             (id, idempotency_key, doc_id, payload, priority, created_at, attempts)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                item.id,
                item.idempotency_key,
                item.doc_id,
                item.payload,
                item.priority.weight(),
                item.created_at.timestamp_micros(),
                item.attempts,
            ],
        );
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
        let conn = self.conn.lock();
        conn.query_row("SELECT COUNT(*) FROM sync_queue", [], |r| r.get(0))
            .unwrap_or(0)
    }

    /// DK-44: 队列快照（只读）——离线队列可视化数据源。
    /// 返回按出队序（优先级 → 入队时序 FIFO）的全部积压项拷贝；
    /// 不改变队列状态。
    pub fn items(&self) -> Vec<QueueItem> {
        let conn = self.conn.lock();
        let mut stmt = match conn.prepare(
            "SELECT id, idempotency_key, doc_id, payload, priority, created_at, attempts
             FROM sync_queue ORDER BY priority ASC, created_at ASC",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = stmt.query_map([], Self::row_to_item);
        match rows {
            Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
            Err(_) => Vec::new(),
        }
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 确认某幂等键对应的操作已成功 (清理索引，允许未来相同语义重新入队)。
    ///
    /// SQLite 版：删除该键所在行（幂等键即行上 UNIQUE 列）。生产流 ack
    /// 均在 dequeue 之后调用（行已离队），此处为防御性清理 + 语义兜底。
    pub fn ack(&self, idempotency_key: &str) -> crate::Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM sync_queue WHERE idempotency_key = ?1",
            rusqlite::params![idempotency_key],
        )
        .map_err(|e| crate::Error::Database(e.to_string()))?;
        Ok(())
    }

    /// 幂等键是否已存在于队列中。
    pub fn contains_key(&self, idempotency_key: &str) -> bool {
        let conn = self.conn.lock();
        conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sync_queue WHERE idempotency_key = ?1)",
            rusqlite::params![idempotency_key],
            |r| r.get::<_, i64>(0),
        )
        .map(|v| v != 0)
        .unwrap_or(false)
    }

    /// 行 → QueueItem 映射（created_at 以微秒精度存取，避免纳秒截断歧义）。
    fn row_to_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<QueueItem> {
        let priority: u8 = row.get(4)?;
        let created_micros: i64 = row.get(5)?;
        Ok(QueueItem {
            id: row.get(0)?,
            idempotency_key: row.get(1)?,
            doc_id: row.get(2)?,
            payload: row.get(3)?,
            priority: Self::priority_from_weight(priority),
            created_at: chrono::DateTime::from_timestamp_micros(created_micros)
                .unwrap_or_else(chrono::Utc::now),
            attempts: row.get::<_, i64>(6).unwrap_or(0).max(0) as u32,
        })
    }

    /// weight(u8) → Priority（越界回退 Medium）。
    fn priority_from_weight(w: u8) -> Priority {
        match w {
            0 => Priority::High,
            2 => Priority::Low,
            _ => Priority::Medium,
        }
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

    // ===== DK-51 案一：落盘持久化（reopen）验证 =====

    fn tmpdir() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("sync_queue.db");
        (dir, path)
    }

    /// DoD：enqueue 落库即持久——写入→杀（drop 连接）→重开→items() 仍在。
    #[test]
    fn test_persist_reopen_items_survive() {
        let (_dir, path) = tmpdir();
        {
            let q = OfflineQueue::open(&path).expect("open");
            q.enqueue(QueueItem::new("doc-a", vec![1], Priority::High))
                .unwrap();
            q.enqueue(QueueItem::new("doc-b", vec![2], Priority::Low))
                .unwrap();
            assert_eq!(q.len(), 2);
        } // drop = 模拟进程退出
        let q2 = OfflineQueue::open(&path).expect("reopen");
        assert_eq!(q2.len(), 2);
        let items = q2.items();
        assert_eq!(items[0].doc_id, "doc-a"); // High 优先
        assert_eq!(items[1].doc_id, "doc-b");
        assert_eq!(items[0].priority, Priority::High);
    }

    /// 幂等键跨进程去重：重开后同键 enqueue 仍被去重。
    #[test]
    fn test_idempotency_survives_reopen() {
        let (_dir, path) = tmpdir();
        {
            let q = OfflineQueue::open(&path).unwrap();
            let item = QueueItem::new("doc", vec![1], Priority::Medium).with_idempotency_key("k1");
            q.enqueue(item).unwrap();
        }
        let q2 = OfflineQueue::open(&path).unwrap();
        let dup = QueueItem::new("doc", vec![9], Priority::High).with_idempotency_key("k1");
        let id = q2.enqueue(dup).unwrap();
        assert_eq!(q2.len(), 1, "同键重开仍去重");
        assert!(q2.contains_key("k1"));
        let back = q2.dequeue().unwrap();
        assert_eq!(back.id, id);
        assert_eq!(back.payload, vec![1], "去重保留的是首次入队项");
    }

    /// DoD：attempts 递增经 requeue 落库——dequeue(1) → requeue → 重开 →
    /// dequeue 得 attempts=2（进程中断不丢重试计数）。
    #[test]
    fn test_attempts_persist_via_requeue_reopen() {
        let (_dir, path) = tmpdir();
        let first_id = {
            let q = OfflineQueue::open(&path).unwrap();
            let item = QueueItem::new("doc", vec![1], Priority::Medium).with_idempotency_key("ra");
            let id = q.enqueue(item).unwrap();
            let out = q.dequeue().unwrap();
            assert_eq!(out.attempts, 1);
            q.requeue(out); // 补发失败回队（attempts=1 落库）
            id
        };
        let q2 = OfflineQueue::open(&path).unwrap();
        assert_eq!(q2.len(), 1, "requeue 行持久");
        let again = q2.dequeue().unwrap();
        assert_eq!(again.id, first_id);
        assert_eq!(again.attempts, 2, "重试计数跨进程累计");
    }

    /// 出队即离队语义持久：dequeue 后（未 requeue）重开 → 项不在
    /// （in-flight 崩溃窗口与旧内存版一致，披露见模块文档）。
    #[test]
    fn test_dequeued_item_not_persisted() {
        let (_dir, path) = tmpdir();
        {
            let q = OfflineQueue::open(&path).unwrap();
            q.enqueue(QueueItem::new("doc", vec![1], Priority::Medium))
                .unwrap();
            assert!(q.dequeue().is_some());
            assert!(q.is_empty());
        }
        let q2 = OfflineQueue::open(&path).unwrap();
        assert!(q2.is_empty(), "已出队项不随重启复活");
    }

    /// items() 快照序稳定：优先级 → 同级入队时序（可视化数据源契约）。
    #[test]
    fn test_items_sorted_by_priority_then_time() {
        let q = OfflineQueue::new();
        q.enqueue(QueueItem::new("doc", vec![1], Priority::Low))
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(2));
        q.enqueue(QueueItem::new("doc", vec![2], Priority::Low))
            .unwrap();
        q.enqueue(QueueItem::new("doc", vec![3], Priority::High))
            .unwrap();
        let items = q.items();
        assert_eq!(items[0].priority, Priority::High);
        assert_eq!(items[1].payload, vec![1], "同优先级 FIFO");
        assert_eq!(items[2].payload, vec![2]);
    }
}
