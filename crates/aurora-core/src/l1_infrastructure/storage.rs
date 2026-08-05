//! 存储层 (SQLite + Sled)
//!
//! 提供关系型数据持久化能力，作为本地数据存储的基石。
//! 底层使用 [rusqlite](https://docs.rs/rusqlite) (bundled SQLite) 与 [sled](https://docs.rs/sled) 实现。

use std::sync::Mutex;

use async_trait::async_trait;
use crate::traits::storage::{Record, Storage, StorageOp, StorageQuery};
use rusqlite::OptionalExtension;

fn base64_encode(input: &[u8]) -> String {
    use std::fmt::Write;
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((input.len() + 2) / 3 * 4);
    for chunk in input.chunks(3) {
        let b = ((chunk[0] as u32) << 16)
            | ((chunk.get(1).copied().unwrap_or(0) as u32) << 8)
            | (chunk.get(2).copied().unwrap_or(0) as u32);
        out.push(CHARS[((b >> 18) & 0x3F) as usize] as char);
        out.push(CHARS[((b >> 12) & 0x3F) as usize] as char);
        out.push(if chunk.len() > 1 {
            CHARS[((b >> 6) & 0x3F) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            CHARS[(b & 0x3F) as usize] as char
        } else {
            '='
        });
    }
    out
}

/// 基于 SQLite 的存储实现。
pub struct SqliteStorage {
    conn: Mutex<rusqlite::Connection>,
}

impl SqliteStorage {
    /// 创建新的 SQLite 存储实例。
    ///
    /// # Arguments
    /// * `path` — SQLite 数据库文件路径。
    pub fn new(path: impl AsRef<std::path::Path>) -> Result<Self, crate::Error> {
        let conn = rusqlite::Connection::open(path)
            .map_err(|e| crate::Error::Database(format!("rusqlite open failed: {}", e)))?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS kv_store (key TEXT PRIMARY KEY, value BLOB)",
            [],
        )
        .map_err(|e| crate::Error::Database(format!("sqlite create table failed: {}", e)))?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// 在内存中创建 SQLite 存储实例（用于测试）。
    pub fn new_in_memory() -> Result<Self, crate::Error> {
        let conn = rusqlite::Connection::open_in_memory()
            .map_err(|e| crate::Error::Database(format!("rusqlite in-memory open failed: {}", e)))?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS kv_store (key TEXT PRIMARY KEY, value BLOB)",
            [],
        )
        .map_err(|e| crate::Error::Database(format!("sqlite create table failed: {}", e)))?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }
}

#[async_trait]
impl Storage for SqliteStorage {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, crate::Error> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| crate::Error::Internal("sqlite storage mutex poisoned".to_string()))?;
        let mut stmt = conn
            .prepare("SELECT value FROM kv_store WHERE key = ?1")
            .map_err(|e| crate::Error::Database(format!("sqlite prepare failed: {}", e)))?;
        let result = stmt
            .query_row(rusqlite::params![key], |row| row.get::<_, Vec<u8>>(0))
            .optional()
            .map_err(|e| crate::Error::Database(format!("sqlite get failed: {}", e)))?;
        Ok(result)
    }

    async fn put(&self, key: &str, value: &[u8]) -> Result<(), crate::Error> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|_| crate::Error::Internal("sqlite storage mutex poisoned".to_string()))?;
        conn.execute(
            "INSERT OR REPLACE INTO kv_store (key, value) VALUES (?1, ?2)",
            rusqlite::params![key, value],
        )
        .map_err(|e| crate::Error::Database(format!("sqlite put failed: {}", e)))?;
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), crate::Error> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|_| crate::Error::Internal("sqlite storage mutex poisoned".to_string()))?;
        conn.execute(
            "DELETE FROM kv_store WHERE key = ?1",
            rusqlite::params![key],
        )
        .map_err(|e| crate::Error::Database(format!("sqlite delete failed: {}", e)))?;
        Ok(())
    }

    async fn query(&self, q: &StorageQuery) -> Result<Vec<Record>, crate::Error> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| crate::Error::Internal("sqlite storage mutex poisoned".to_string()))?;

        let mut sql = format!("SELECT * FROM {}", q.table);
        let mut conditions: Vec<String> = vec![];
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![];

        for filter in &q.filters {
            let condition = match filter.op.as_str() {
                "eq" => format!("{} = ?", filter.field),
                "ne" => format!("{} != ?", filter.field),
                "gt" => format!("{} > ?", filter.field),
                "gte" => format!("{} >= ?", filter.field),
                "lt" => format!("{} < ?", filter.field),
                "lte" => format!("{} <= ?", filter.field),
                "like" => format!("{} LIKE ?", filter.field),
                _ => format!("{} = ?", filter.field),
            };
            conditions.push(condition);
            let value_str = match &filter.value {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            params.push(Box::new(value_str));
        }

        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }

        if let Some(ref order_by) = q.order_by {
            sql.push_str(&format!(" ORDER BY {}", order_by));
        }

        if let Some(limit) = q.limit {
            sql.push_str(&format!(" LIMIT {}", limit));
        }

        if let Some(offset) = q.offset {
            sql.push_str(&format!(" OFFSET {}", offset));
        }

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| crate::Error::Database(format!("sqlite query prepare failed: {}", e)))?;

        let column_count = stmt.column_count();
        let column_names: Vec<String> = (0..column_count)
            .map(|i| stmt.column_name(i).unwrap_or("").to_string())
            .collect();

        let rows = stmt
            .query_map(rusqlite::params_from_iter(params.iter()), |row| {
                let mut map = serde_json::Map::new();
                for (i, name) in column_names.iter().enumerate() {
                    let value: rusqlite::types::Value = row.get(i).unwrap_or(rusqlite::types::Value::Null);
                    let json_value = match value {
                        rusqlite::types::Value::Null => serde_json::Value::Null,
                        rusqlite::types::Value::Integer(v) => serde_json::Value::Number(v.into()),
                        rusqlite::types::Value::Real(v) => {
                            serde_json::Number::from_f64(v)
                                .map(serde_json::Value::Number)
                                .unwrap_or(serde_json::Value::Null)
                        }
                        rusqlite::types::Value::Text(v) => serde_json::Value::String(v),
                        rusqlite::types::Value::Blob(v) => {
                            serde_json::Value::String(base64_encode(&v))
                        }
                    };
                    map.insert(name.clone(), json_value);
                }
                Ok(Record {
                    data: serde_json::Value::Object(map),
                })
            })
            .map_err(|e| crate::Error::Database(format!("sqlite query failed: {}", e)))?;

        let records: Vec<Record> = rows.filter_map(|r| r.ok()).collect();
        Ok(records)
    }

    async fn transaction(&self, ops: &[StorageOp]) -> Result<(), crate::Error> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|_| crate::Error::Internal("sqlite storage mutex poisoned".to_string()))?;
        let tx = conn
            .transaction()
            .map_err(|e| crate::Error::Database(format!("sqlite transaction begin failed: {}", e)))?;
        for op in ops {
            match op {
                StorageOp::Put { key, value } => {
                    tx.execute(
                        "INSERT OR REPLACE INTO kv_store (key, value) VALUES (?1, ?2)",
                        rusqlite::params![key, value.as_slice()],
                    )
                    .map_err(|e| crate::Error::Database(format!("sqlite put failed: {}", e)))?;
                }
                StorageOp::Delete { key } => {
                    tx.execute(
                        "DELETE FROM kv_store WHERE key = ?1",
                        rusqlite::params![key],
                    )
                    .map_err(|e| crate::Error::Database(format!("sqlite delete failed: {}", e)))?;
                }
            }
        }
        tx.commit()
            .map_err(|e| crate::Error::Database(format!("sqlite transaction commit failed: {}", e)))?;
        Ok(())
    }
}

/// 基于 Sled 的嵌入式 KV 存储实现。
pub struct SledStorage {
    db: sled::Db,
}

impl SledStorage {
    /// 创建新的 Sled 存储实例。
    ///
    /// # Arguments
    /// * `path` — Sled 数据库目录路径。
    pub fn new(path: impl AsRef<std::path::Path>) -> Result<Self, crate::Error> {
        let db = sled::open(path)
            .map_err(|e| crate::Error::Database(format!("sled open failed: {}", e)))?;
        Ok(Self { db })
    }
}

#[async_trait]
impl Storage for SledStorage {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, crate::Error> {
        let result = self
            .db
            .get(key)
            .map_err(|e| crate::Error::Database(format!("sled get failed: {}", e)))?;
        Ok(result.map(|ivec| ivec.to_vec()))
    }

    async fn put(&self, key: &str, value: &[u8]) -> Result<(), crate::Error> {
        self.db
            .insert(key, value)
            .map_err(|e| crate::Error::Database(format!("sled put failed: {}", e)))?;
        self.db
            .flush()
            .map_err(|e| crate::Error::Database(format!("sled flush failed: {}", e)))?;
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), crate::Error> {
        self.db
            .remove(key)
            .map_err(|e| crate::Error::Database(format!("sled delete failed: {}", e)))?;
        self.db
            .flush()
            .map_err(|e| crate::Error::Database(format!("sled flush failed: {}", e)))?;
        Ok(())
    }

    async fn query(&self, q: &StorageQuery) -> Result<Vec<Record>, crate::Error> {
        // Sled 是纯 KV 存储，不支持关系型查询。
        // 回退到遍历所有键值对并在内存中过滤。
        let mut records = vec![];
        for item in self.db.iter() {
            let (key, value) = item.map_err(|e| crate::Error::Database(format!("sled iter failed: {}", e)))?;
            let key_str = String::from_utf8_lossy(&key);
            // 仅返回以 table 名为前缀的键作为简单过滤策略
            if !key_str.starts_with(&format!("{}:", q.table)) {
                continue;
            }
            let data = serde_json::json!({
                "key": key_str.to_string(),
                "value": base64_encode(&value),
            });
            records.push(Record { data });
        }

        if let Some(limit) = q.limit {
            records.truncate(limit);
        }
        Ok(records)
    }

    async fn transaction(&self, ops: &[StorageOp]) -> Result<(), crate::Error> {
        // sled 的单个 insert/remove 是原子性的，但批量操作需要应用层保证。
        // 先顺序执行，最后统一 flush。
        for op in ops {
            match op {
                StorageOp::Put { key, value } => {
                    self.db
                        .insert(key.as_str(), value.as_slice())
                        .map_err(|e| crate::Error::Database(format!("sled put failed: {}", e)))?;
                }
                StorageOp::Delete { key } => {
                    self.db
                        .remove(key.as_str())
                        .map_err(|e| crate::Error::Database(format!("sled delete failed: {}", e)))?;
                }
            }
        }
        self.db
            .flush()
            .map_err(|e| crate::Error::Database(format!("sled flush failed: {}", e)))?;
        Ok(())
    }
}
