//! 向量数据库 (基于 LanceDB + SQLite-vec)
//!
//! 提供向量存储与相似度检索能力，支撑语义搜索与 RAG 检索增强生成。
//! 底层使用 [LanceDB](https://lancedb.github.io/lancedb/) 与 SQLite vec 扩展实现。

use async_trait::async_trait;
use std::sync::{Arc, Mutex};

use crate::traits::vector_store::{QueryFilter, SearchResult, VectorStore};

/// 基于 LanceDB 的向量存储实现。
///
/// 使用 `tokio::runtime::Runtime` 在同步接口中驱动 LanceDB 的异步 API。
#[allow(dead_code)]
pub struct LanceDbStore {
    uri: String,
    table_name: String,
    rt: Arc<tokio::runtime::Runtime>,
}

impl LanceDbStore {
    /// 创建新的 LanceDB 存储实例。
    ///
    /// # Arguments
    /// * `uri` — LanceDB 数据库 URI，如 `./data/lancedb`。
    /// * `table_name` — 默认表名。
    pub fn new(
        uri: impl Into<String>,
        table_name: impl Into<String>,
    ) -> Result<Self, crate::Error> {
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| crate::Error::Internal(format!("tokio runtime creation failed: {}", e)))?;
        Ok(Self {
            uri: uri.into(),
            table_name: table_name.into(),
            rt: Arc::new(rt),
        })
    }
}

#[async_trait]
impl VectorStore for LanceDbStore {
    async fn add(
        &self,
        id: &str,
        vector: &[f32],
        metadata: &serde_json::Value,
    ) -> Result<(), crate::Error> {
        tracing::debug!(
            "lancedb add: id={}, vector_len={}, metadata={}",
            id,
            vector.len(),
            metadata
        );
        // TODO: 接入 lancedb 真实异步 API (connect -> open_table -> add)。
        Ok(())
    }

    async fn search(
        &self,
        query: &[f32],
        top_k: usize,
        _filter: Option<&QueryFilter>,
    ) -> Result<Vec<SearchResult>, crate::Error> {
        tracing::debug!(
            "lancedb search: vector_len={}, top_k={}",
            query.len(),
            top_k
        );
        // TODO: 接入 lancedb 真实向量搜索 API。
        Ok(vec![])
    }

    async fn delete(&self, id: &str) -> Result<(), crate::Error> {
        tracing::debug!("lancedb delete: id={}", id);
        // TODO: 接入 lancedb 真实删除 API。
        Ok(())
    }

    async fn hybrid_search(
        &self,
        text_query: &str,
        vector: &[f32],
        alpha: f32,
    ) -> Result<Vec<SearchResult>, crate::Error> {
        tracing::debug!(
            "lancedb hybrid_search: text={}, vector_len={}, alpha={}",
            text_query,
            vector.len(),
            alpha
        );
        // TODO: 接入 lancedb 混合搜索 API (向量 + 全文)。
        Ok(vec![])
    }
}

/// 基于 SQLite + sqlite-vec 扩展的向量存储实现。
pub struct SqliteVecStore {
    conn: Mutex<rusqlite::Connection>,
    table_name: String,
    dimension: usize,
}

impl SqliteVecStore {
    /// 创建新的 SQLite 向量存储实例。
    ///
    /// # Arguments
    /// * `path` — SQLite 数据库文件路径。
    /// * `table_name` — 向量表名。
    /// * `dimension` — 向量维度。
    pub fn new(
        path: impl AsRef<std::path::Path>,
        table_name: impl Into<String>,
        dimension: usize,
    ) -> Result<Self, crate::Error> {
        let conn = rusqlite::Connection::open(path)
            .map_err(|e| crate::Error::Database(format!("rusqlite open failed: {}", e)))?;
        let table_name = table_name.into();
        // 表名将拼入 DDL 字符串，必须限制为安全字符集，防止 SQL 注入。
        if table_name.is_empty()
            || !table_name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return Err(crate::Error::InvalidInput(format!(
                "invalid vector table name: {:?} (allowed: [A-Za-z0-9_])",
                table_name
            )));
        }
        // sqlite-vec 扩展为可选能力：加载失败仅降级为无向量索引加速，
        // 不阻断建表；但建表失败必须报错——此前整个 execute_batch 被
        // `let _ =` 吞错，会导致构造返回 Ok 但表实际不存在。
        let _ = conn.execute_batch("SELECT load_extension('sqlite_vec');");
        conn.execute_batch(&format!(
            "CREATE TABLE IF NOT EXISTS {} (
                 id TEXT PRIMARY KEY,
                 vec BLOB,
                 metadata TEXT
             );",
            table_name
        ))
        .map_err(|e| crate::Error::Database(format!("create vector table failed: {}", e)))?;
        Ok(Self {
            conn: Mutex::new(conn),
            table_name,
            dimension,
        })
    }
}

#[async_trait]
impl VectorStore for SqliteVecStore {
    async fn add(
        &self,
        id: &str,
        vector: &[f32],
        metadata: &serde_json::Value,
    ) -> Result<(), crate::Error> {
        if vector.len() != self.dimension {
            return Err(crate::Error::InvalidInput(format!(
                "vector dimension mismatch: expected {}, got {}",
                self.dimension,
                vector.len()
            )));
        }
        let conn = self
            .conn
            .lock()
            .map_err(|_| crate::Error::Internal("sqlite vec mutex poisoned".to_string()))?;
        let vec_bytes = vector
            .iter()
            .flat_map(|f| f.to_ne_bytes())
            .collect::<Vec<u8>>();
        let meta_str = metadata.to_string();
        conn.execute(
            &format!(
                "INSERT OR REPLACE INTO {} (id, vec, metadata) VALUES (?1, ?2, ?3)",
                self.table_name
            ),
            rusqlite::params![id, vec_bytes, meta_str],
        )
        .map_err(|e| crate::Error::Database(format!("sqlite vec insert failed: {}", e)))?;
        Ok(())
    }

    async fn search(
        &self,
        query: &[f32],
        top_k: usize,
        filter: Option<&QueryFilter>,
    ) -> Result<Vec<SearchResult>, crate::Error> {
        if query.len() != self.dimension {
            return Err(crate::Error::InvalidInput(format!(
                "query dimension mismatch: expected {}, got {}",
                self.dimension,
                query.len()
            )));
        }
        let conn = self
            .conn
            .lock()
            .map_err(|_| crate::Error::Internal("sqlite vec mutex poisoned".to_string()))?;

        let mut sql = format!("SELECT id, vec, metadata FROM {}", self.table_name);
        let params: Vec<Box<dyn rusqlite::ToSql>> = vec![];

        if let Some(f) = filter {
            sql.push_str(&format!(" WHERE metadata LIKE '%{}%'", f.value));
        }

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| crate::Error::Database(format!("sqlite vec prepare failed: {}", e)))?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(params.iter()), |row| {
                let id: String = row.get(0)?;
                let vec_bytes: Vec<u8> = row.get(1)?;
                let metadata_str: String = row.get(2)?;
                let vector: Vec<f32> = vec_bytes
                    .chunks_exact(4)
                    .map(|b| f32::from_ne_bytes([b[0], b[1], b[2], b[3]]))
                    .collect();
                let metadata: serde_json::Value =
                    serde_json::from_str(&metadata_str).unwrap_or(serde_json::Value::Null);
                let score = cosine_similarity(query, &vector);
                Ok(SearchResult {
                    id,
                    score,
                    metadata,
                })
            })
            .map_err(|e| crate::Error::Database(format!("sqlite vec query failed: {}", e)))?;

        let mut results: Vec<SearchResult> = rows.filter_map(|r| r.ok()).collect();
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(top_k);
        Ok(results)
    }

    async fn delete(&self, id: &str) -> Result<(), crate::Error> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| crate::Error::Internal("sqlite vec mutex poisoned".to_string()))?;
        conn.execute(
            &format!("DELETE FROM {} WHERE id = ?1", self.table_name),
            rusqlite::params![id],
        )
        .map_err(|e| crate::Error::Database(format!("sqlite vec delete failed: {}", e)))?;
        Ok(())
    }

    async fn hybrid_search(
        &self,
        text_query: &str,
        vector: &[f32],
        alpha: f32,
    ) -> Result<Vec<SearchResult>, crate::Error> {
        tracing::debug!(
            "sqlitevec hybrid_search: text={}, vector_len={}, alpha={}",
            text_query,
            vector.len(),
            alpha
        );
        // SQLite-vec 暂不支持原生全文混合搜索，回退到纯向量搜索。
        self.search(vector, 10, None).await
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}
