//! 查询引擎 (Query Engine)
//!
//! 提供统一的 Query DSL、基于成本的查询优化器、以及自适应 LRU 缓存层。
//!
//! ## 架构
//! - **Query DSL**: JSON 友好的声明式查询语言，支持过滤、排序、分页、聚合。
//! - **Query Optimizer**: 基于成本模型自动选择 Tantivy / LanceDB / SQLite 执行路径。
//! - **Query Cache**: LRU 缓存，TTL 根据数据变更频率动态调整。

use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{debug, trace, warn};

use crate::traits::storage::{QueryFilter as StorageFilter, Storage, StorageQuery};
use crate::traits::vector_store::{
    QueryFilter as VectorFilter, VectorStore,
};

// ==================== Query DSL ====================

/// 统一查询对象（JSON 友好）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Query {
    /// 查询目标（表 / 集合 / 索引名称）
    pub source: String,
    /// 过滤条件
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<Filter>,
    /// 排序规则
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sort: Vec<Sort>,
    /// 分页参数
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pagination: Option<Pagination>,
    /// 聚合规则
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aggregation: Option<Aggregation>,
    /// 返回字段（None 表示返回全部）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub projection: Option<Vec<String>>,
}

/// 过滤条件（支持嵌套逻辑与字段条件）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "op")]
pub enum Filter {
    And {
        filters: Vec<Filter>,
    },
    Or {
        filters: Vec<Filter>,
    },
    Not {
        filter: Box<Filter>,
    },
    Eq {
        field: String,
        value: serde_json::Value,
    },
    Ne {
        field: String,
        value: serde_json::Value,
    },
    Gt {
        field: String,
        value: serde_json::Value,
    },
    Gte {
        field: String,
        value: serde_json::Value,
    },
    Lt {
        field: String,
        value: serde_json::Value,
    },
    Lte {
        field: String,
        value: serde_json::Value,
    },
    Contains {
        field: String,
        value: String,
    },
    StartsWith {
        field: String,
        value: String,
    },
    In {
        field: String,
        values: Vec<serde_json::Value>,
    },
    FullText {
        query: String,
        fields: Option<Vec<String>>,
    },
    Vector {
        vector: Vec<f32>,
        top_k: usize,
    },
}

/// 排序规则
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sort {
    pub field: String,
    pub direction: SortDirection,
}

/// 排序方向
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortDirection {
    Asc,
    Desc,
}

/// 分页参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pagination {
    pub limit: usize,
    pub offset: usize,
}

/// 聚合规则
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum Aggregation {
    Count {
        field: Option<String>,
        alias: Option<String>,
    },
    Sum {
        field: String,
        alias: Option<String>,
    },
    Avg {
        field: String,
        alias: Option<String>,
    },
    Min {
        field: String,
        alias: Option<String>,
    },
    Max {
        field: String,
        alias: Option<String>,
    },
    GroupBy {
        field: String,
        aggs: Vec<Aggregation>,
    },
}

/// 查询结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    pub items: Vec<serde_json::Value>,
    pub total: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aggregations: Option<HashMap<String, serde_json::Value>>,
}

// ==================== Execution Plan & Optimizer ====================

/// 执行路径
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionPath {
    Sqlite,
    Tantivy,
    LanceDb,
    Hybrid,
}

/// 执行计划
#[derive(Debug, Clone)]
pub struct ExecutionPlan {
    pub path: ExecutionPath,
    pub estimated_cost: f64,
    pub query: Query,
}

/// 基于成本的查询优化器
#[derive(Debug, Clone, Default)]
pub struct QueryOptimizer;

impl QueryOptimizer {
    pub fn new() -> Self {
        Self
    }

    /// 为给定查询生成最优执行计划
    pub fn optimize(&self, query: &Query) -> ExecutionPlan {
        let candidates = [
            ExecutionPath::Sqlite,
            ExecutionPath::Tantivy,
            ExecutionPath::LanceDb,
            ExecutionPath::Hybrid,
        ];

        let mut best_path = ExecutionPath::Sqlite;
        let mut best_cost = f64::MAX;

        for path in candidates {
            if self.is_applicable(query, path) {
                let cost = self.estimate_cost(query, path);
                trace!(
                    "Cost for path {:?} on source '{}': {:.2}",
                    path,
                    query.source,
                    cost
                );
                if cost < best_cost {
                    best_cost = cost;
                    best_path = path;
                }
            }
        }

        debug!(
            "Selected path {:?} for source '{}' with estimated cost {:.2}",
            best_path, query.source, best_cost
        );

        ExecutionPlan {
            path: best_path,
            estimated_cost: best_cost,
            query: query.clone(),
        }
    }

    fn is_applicable(&self, query: &Query, path: ExecutionPath) -> bool {
        match path {
            ExecutionPath::Sqlite => {
                // SQLite 适合纯结构化查询，不包含全文或向量条件
                !has_fulltext(query) && !has_vector(query)
            }
            ExecutionPath::Tantivy => has_fulltext(query) && !has_vector(query),
            ExecutionPath::LanceDb => has_vector(query) && !has_fulltext(query),
            ExecutionPath::Hybrid => has_fulltext(query) && has_vector(query),
        }
    }

    fn estimate_cost(&self, query: &Query, path: ExecutionPath) -> f64 {
        match path {
            ExecutionPath::Sqlite => estimate_sqlite_cost(query),
            ExecutionPath::Tantivy => estimate_tantivy_cost(query),
            ExecutionPath::LanceDb => estimate_lancedb_cost(query),
            ExecutionPath::Hybrid => estimate_hybrid_cost(query),
        }
    }
}

fn has_fulltext(query: &Query) -> bool {
    match &query.filter {
        Some(f) => filter_contains_fulltext(f),
        None => false,
    }
}

fn filter_contains_fulltext(filter: &Filter) -> bool {
    match filter {
        Filter::FullText { .. } => true,
        Filter::And { filters } | Filter::Or { filters } => {
            filters.iter().any(filter_contains_fulltext)
        }
        Filter::Not { filter } => filter_contains_fulltext(filter),
        _ => false,
    }
}

fn has_vector(query: &Query) -> bool {
    match &query.filter {
        Some(f) => filter_contains_vector(f),
        None => false,
    }
}

fn filter_contains_vector(filter: &Filter) -> bool {
    match filter {
        Filter::Vector { .. } => true,
        Filter::And { filters } | Filter::Or { filters } => {
            filters.iter().any(filter_contains_vector)
        }
        Filter::Not { filter } => filter_contains_vector(filter),
        _ => false,
    }
}

#[allow(dead_code)]
fn has_only_fulltext(query: &Query) -> bool {
    match &query.filter {
        Some(f) => filter_is_only_fulltext(f),
        None => false,
    }
}

#[allow(dead_code)]
fn filter_is_only_fulltext(filter: &Filter) -> bool {
    match filter {
        Filter::FullText { .. } => true,
        Filter::And { filters } | Filter::Or { filters } => {
            filters.iter().all(filter_is_only_fulltext)
        }
        _ => false,
    }
}

#[allow(dead_code)]
fn has_only_vector(query: &Query) -> bool {
    match &query.filter {
        Some(f) => filter_is_only_vector(f),
        None => false,
    }
}

#[allow(dead_code)]
fn filter_is_only_vector(filter: &Filter) -> bool {
    match filter {
        Filter::Vector { .. } => true,
        Filter::And { filters } | Filter::Or { filters } => {
            filters.iter().all(filter_is_only_vector)
        }
        _ => false,
    }
}

fn count_conditions(filter: &Option<Filter>) -> usize {
    match filter {
        Some(f) => count_filter_nodes(f),
        None => 0,
    }
}

fn count_filter_nodes(filter: &Filter) -> usize {
    match filter {
        Filter::And { filters } | Filter::Or { filters } => {
            1 + filters.iter().map(count_filter_nodes).sum::<usize>()
        }
        Filter::Not { filter } => 1 + count_filter_nodes(filter),
        _ => 1,
    }
}

fn count_text_terms(filter: &Option<Filter>) -> usize {
    match filter {
        Some(f) => count_text_terms_inner(f),
        None => 0,
    }
}

fn count_text_terms_inner(filter: &Filter) -> usize {
    match filter {
        Filter::FullText { query, .. } => query.split_whitespace().count().max(1),
        Filter::And { filters } | Filter::Or { filters } => {
            filters.iter().map(count_text_terms_inner).sum()
        }
        Filter::Not { filter } => count_text_terms_inner(filter),
        _ => 0,
    }
}

fn get_vector_top_k(filter: &Option<Filter>) -> Option<usize> {
    match filter {
        Some(f) => get_vector_top_k_inner(f),
        None => None,
    }
}

fn get_vector_top_k_inner(filter: &Filter) -> Option<usize> {
    match filter {
        Filter::Vector { top_k, .. } => Some(*top_k),
        Filter::And { filters } | Filter::Or { filters } => {
            filters.iter().filter_map(get_vector_top_k_inner).next()
        }
        Filter::Not { filter } => get_vector_top_k_inner(filter),
        _ => None,
    }
}

fn estimate_sqlite_cost(query: &Query) -> f64 {
    let base = 100.0;
    let filter_penalty = count_conditions(&query.filter) as f64 * 25.0;
    let sort_penalty = query.sort.len() as f64 * 30.0;
    let agg_penalty = if query.aggregation.is_some() {
        80.0
    } else {
        0.0
    };
    let pagination_penalty = query
        .pagination
        .as_ref()
        .map(|p| p.offset as f64 * 0.5)
        .unwrap_or(0.0);
    base + filter_penalty + sort_penalty + agg_penalty + pagination_penalty
}

fn estimate_tantivy_cost(query: &Query) -> f64 {
    let base = 60.0;
    let term_count = count_text_terms(&query.filter);
    base + term_count as f64 * 20.0
}

fn estimate_lancedb_cost(query: &Query) -> f64 {
    let base = 50.0;
    let top_k = get_vector_top_k(&query.filter).unwrap_or(10);
    base + top_k as f64 * 3.0
}

fn estimate_hybrid_cost(query: &Query) -> f64 {
    estimate_tantivy_cost(query) + estimate_lancedb_cost(query) + 180.0
}

// ==================== LRU Cache ====================

/// 自适应 LRU 查询缓存
pub struct QueryCache {
    cache: Arc<RwLock<LruCache<String, CacheEntry>>>,
    stats: Arc<RwLock<CacheStats>>,
    default_capacity: usize,
}

#[derive(Debug, Clone)]
struct CacheEntry {
    result: QueryResult,
    created_at: Instant,
    ttl: Duration,
    access_count: u64,
}

#[derive(Debug, Clone, Default)]
struct CacheStats {
    hit_count: u64,
    miss_count: u64,
    evict_count: u64,
    /// 各 source 的变更频率（次 / 分钟，EMA）
    change_frequency: HashMap<String, f64>,
    /// 各 source 上次变更时间
    last_change_time: HashMap<String, Instant>,
}

impl QueryCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            cache: Arc::new(RwLock::new(LruCache::new(capacity))),
            stats: Arc::new(RwLock::new(CacheStats::default())),
            default_capacity: capacity,
        }
    }

    /// 获取缓存结果（如果存在且未过期）
    pub fn get(&self, key: &str, _source: &str) -> Option<QueryResult> {
        let mut cache = self.cache.write();
        let entry = match cache.get_mut(&key.to_string()) {
            Some(e) => e,
            None => {
                self.stats.write().miss_count += 1;
                return None;
            }
        };

        if entry.created_at.elapsed() > entry.ttl {
            cache.remove(&key.to_string());
            self.stats.write().evict_count += 1;
            return None;
        }

        entry.access_count += 1;
        self.stats.write().hit_count += 1;
        Some(entry.result.clone())
    }

    /// 写入缓存
    pub fn put(&self, key: String, _source: &str, result: QueryResult, ttl: Duration) {
        let entry = CacheEntry {
            result,
            created_at: Instant::now(),
            ttl,
            access_count: 1,
        };
        let mut cache = self.cache.write();
        let prev = cache.put(key, entry);
        if prev.is_some() {
            self.stats.write().evict_count += 1;
        }
    }

    /// 使某个 source 下的所有缓存失效（简化：全部清空）
    ///
    /// 后续可按 source 建立二级索引实现精确失效。
    pub fn invalidate(&self, source: &str) {
        let mut cache = self.cache.write();
        let before = cache.len();
        cache.clear();
        let after = cache.len();
        if before > after {
            debug!(
                "Invalidated {} cache entries for source '{}'",
                before - after,
                source
            );
        }
    }

    /// 记录 source 的数据变更，用于调整 TTL
    pub fn record_change(&self, source: &str) {
        let mut stats = self.stats.write();
        let now = Instant::now();

        if let Some(last_time) = stats.last_change_time.get(source) {
            let elapsed_secs = now.duration_since(*last_time).as_secs_f64();
            if elapsed_secs > 0.0 {
                let instant_freq = 60.0 / elapsed_secs; // changes per minute
                let old_freq = stats.change_frequency.get(source).copied().unwrap_or(0.0);
                let alpha = 0.3; // EMA smoothing factor
                let new_freq = alpha * instant_freq + (1.0 - alpha) * old_freq;
                stats.change_frequency.insert(source.to_string(), new_freq);
                trace!(
                    "Source '{}' change frequency updated: {:.2} changes/min",
                    source,
                    new_freq
                );
            }
        }

        stats.last_change_time.insert(source.to_string(), now);
    }

    /// 基于数据变更频率计算 TTL
    pub fn compute_ttl(&self, source: &str) -> Duration {
        let stats = self.stats.read();
        let freq = stats.change_frequency.get(source).copied().unwrap_or(0.0);

        let secs = if freq > 100.0 {
            5
        } else if freq > 30.0 {
            15
        } else if freq > 10.0 {
            60
        } else if freq > 1.0 {
            300
        } else if freq > 0.1 {
            1800
        } else {
            3600
        };

        Duration::from_secs(secs)
    }

    /// 获取缓存统计
    pub fn stats(&self) -> CacheMetrics {
        let s = self.stats.read();
        CacheMetrics {
            hit_count: s.hit_count,
            miss_count: s.miss_count,
            evict_count: s.evict_count,
            size: self.cache.read().len(),
            capacity: self.default_capacity,
        }
    }
}

/// 缓存指标
#[derive(Debug, Clone, Copy)]
pub struct CacheMetrics {
    pub hit_count: u64,
    pub miss_count: u64,
    pub evict_count: u64,
    pub size: usize,
    pub capacity: usize,
}

/// 简易 LRU 缓存（基于 HashMap + BTreeMap 实现 O(log n) 操作）
struct LruCache<K, V> {
    values: HashMap<K, (V, u64)>,
    access_order: BTreeMap<u64, K>,
    tick: u64,
    capacity: usize,
}

impl<K: Clone + Eq + std::hash::Hash, V> LruCache<K, V> {
    fn new(capacity: usize) -> Self {
        Self {
            values: HashMap::with_capacity(capacity.saturating_add(1)),
            access_order: BTreeMap::new(),
            tick: 0,
            capacity,
        }
    }

    fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        let (value, old_tick) = self.values.get_mut(key)?;
        let old_tick_val = *old_tick;
        let new_tick = self.tick.wrapping_add(1);
        self.tick = new_tick;
        self.access_order.remove(&old_tick_val);
        self.access_order.insert(new_tick, key.clone());
        *old_tick = new_tick;
        Some(value)
    }

    fn put(&mut self, key: K, value: V) -> Option<V> {
        self.tick = self.tick.wrapping_add(1);
        let tick = self.tick;

        let evicted = if self.values.contains_key(&key) {
            if let Some((_, old_tick)) = self.values.get(&key) {
                let old_tick = *old_tick;
                self.access_order.remove(&old_tick);
            }
            None
        } else if self.values.len() >= self.capacity {
            if let Some((oldest_tick, oldest_key)) = self.access_order.iter().next() {
                let oldest_key = oldest_key.clone();
                let oldest_tick = *oldest_tick;
                let evicted = self.values.remove(&oldest_key).map(|(v, _)| v);
                self.access_order.remove(&oldest_tick);
                evicted
            } else {
                None
            }
        } else {
            None
        };

        self.values.insert(key.clone(), (value, tick));
        self.access_order.insert(tick, key);
        evicted
    }

    fn remove(&mut self, key: &K) -> Option<V> {
        let (value, tick) = self.values.remove(key)?;
        self.access_order.remove(&tick);
        Some(value)
    }

    fn len(&self) -> usize {
        self.values.len()
    }

    fn clear(&mut self) {
        self.values.clear();
        self.access_order.clear();
    }
}

// ==================== FullText Backend Trait ====================

/// 全文搜索结果
#[derive(Debug, Clone)]
pub struct FullTextResult {
    pub id: String,
    pub score: f32,
    pub data: Option<serde_json::Value>,
    pub highlights: Vec<String>,
}

/// 全文搜索后端抽象
pub trait FullTextSearch: Send + Sync {
    fn search(
        &self,
        query: &str,
        fields: Option<&[String]>,
        limit: usize,
    ) -> Result<Vec<FullTextResult>, crate::Error>;
}

// ==================== Query Engine ====================

/// 查询引擎
///
/// 整合 Query DSL、成本优化器与自适应缓存，
/// 自动路由到 Tantivy / LanceDB / SQLite 执行。
pub struct QueryEngine {
    optimizer: QueryOptimizer,
    cache: QueryCache,
    storage: Option<Arc<dyn Storage>>,
    vector_store: Option<Arc<dyn VectorStore>>,
    fulltext_search: Option<Arc<dyn FullTextSearch>>,
}

impl QueryEngine {
    pub fn new() -> Self {
        Self {
            optimizer: QueryOptimizer::new(),
            cache: QueryCache::new(1000),
            storage: None,
            vector_store: None,
            fulltext_search: None,
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            optimizer: QueryOptimizer::new(),
            cache: QueryCache::new(capacity),
            storage: None,
            vector_store: None,
            fulltext_search: None,
        }
    }

    pub fn with_storage(mut self, storage: Arc<dyn Storage>) -> Self {
        self.storage = Some(storage);
        self
    }

    pub fn with_vector_store(mut self, store: Arc<dyn VectorStore>) -> Self {
        self.vector_store = Some(store);
        self
    }

    pub fn with_fulltext_search(mut self, search: Arc<dyn FullTextSearch>) -> Self {
        self.fulltext_search = Some(search);
        self
    }

    /// 执行查询（自动优化、缓存）
    pub async fn execute(&self, query: &Query) -> Result<QueryResult, crate::Error> {
        let plan = self.optimizer.optimize(query);

        let cache_key = match serde_json::to_string(query) {
            Ok(s) => s,
            Err(e) => {
                warn!("Failed to serialize query for cache key: {}", e);
                format!("{:?}", query)
            }
        };

        if let Some(cached) = self.cache.get(&cache_key, &query.source) {
            debug!("Cache hit for query on source '{}'", query.source);
            return Ok(cached);
        }

        self.cache.stats.write().miss_count += 1;

        trace!(
            "Executing query on source '{}' via {:?}",
            query.source,
            plan.path
        );

        let mut result = match plan.path {
            ExecutionPath::Sqlite => self.execute_sqlite(query).await?,
            ExecutionPath::Tantivy => self.execute_tantivy(query).await?,
            ExecutionPath::LanceDb => self.execute_lancedb(query).await?,
            ExecutionPath::Hybrid => self.execute_hybrid(query).await?,
        };

        // 应用投影
        if let Some(ref projection) = query.projection {
            apply_projection(&mut result, projection);
        }

        let ttl = self.cache.compute_ttl(&query.source);
        self.cache
            .put(cache_key, &query.source, result.clone(), ttl);

        Ok(result)
    }

    /// 手动使某 source 的缓存失效
    pub fn invalidate(&self, source: &str) {
        self.cache.invalidate(source);
        self.cache.record_change(source);
    }

    /// 上报数据变更（用于调整 TTL）
    pub fn record_change(&self, source: &str) {
        self.cache.record_change(source);
    }

    /// 获取缓存指标
    pub fn cache_metrics(&self) -> CacheMetrics {
        self.cache.stats()
    }

    async fn execute_sqlite(&self, query: &Query) -> Result<QueryResult, crate::Error> {
        let storage = self.storage.as_ref().ok_or_else(|| {
            crate::Error::InvalidInput("SQLite storage backend not configured".to_string())
        })?;

        let storage_query = convert_to_storage_query(query)?;
        let records = storage.query(&storage_query).await?;

        let mut items: Vec<serde_json::Value> = records.into_iter().map(|r| r.data).collect();

        // 内存排序（如果 SQLite 未能完全处理）
        apply_sort(&mut items, &query.sort);

        let total = items.len() + query.pagination.as_ref().map(|p| p.offset).unwrap_or(0);

        let items = apply_pagination(items, &query.pagination);

        let aggregations = if let Some(ref agg) = query.aggregation {
            let mut map = HashMap::new();
            map.insert(agg_key(agg), execute_aggregation(&items, agg)?);
            Some(map)
        } else {
            None
        };

        Ok(QueryResult {
            items,
            total,
            aggregations,
        })
    }

    async fn execute_tantivy(&self, query: &Query) -> Result<QueryResult, crate::Error> {
        let fulltext = self.fulltext_search.as_ref().ok_or_else(|| {
            crate::Error::InvalidInput("Tantivy fulltext backend not configured".to_string())
        })?;

        let (text_query, fields, limit) = extract_fulltext_params(query)?;
        let results = fulltext.search(&text_query, fields.as_deref(), limit)?;

        let mut items: Vec<serde_json::Value> = results
            .into_iter()
            .map(|r| {
                let mut map = serde_json::Map::new();
                map.insert("id".to_string(), serde_json::Value::String(r.id));
                map.insert("score".to_string(), serde_json::Value::from(r.score as f64));
                if let Some(data) = r.data {
                    if let serde_json::Value::Object(m) = data {
                        for (k, v) in m {
                            map.insert(k, v);
                        }
                    }
                }
                serde_json::Value::Object(map)
            })
            .collect();

        apply_sort(&mut items, &query.sort);

        let total = items.len();
        let items = apply_pagination(items, &query.pagination);

        Ok(QueryResult {
            items,
            total,
            aggregations: None,
        })
    }

    async fn execute_lancedb(&self, query: &Query) -> Result<QueryResult, crate::Error> {
        let vector_store = self.vector_store.as_ref().ok_or_else(|| {
            crate::Error::InvalidInput("LanceDB vector backend not configured".to_string())
        })?;

        let (vector, top_k) = extract_vector_params(query)?;
        let filter = convert_filter_to_vector_filter(&query.filter);

        // LanceDB 的 top_k 需要包含 offset
        let adjusted_top_k = query
            .pagination
            .as_ref()
            .map(|p| p.offset + top_k)
            .unwrap_or(top_k);

        let results = vector_store.search(&vector, adjusted_top_k, filter.as_ref()).await?;

        let mut items: Vec<serde_json::Value> = results
            .into_iter()
            .map(|r| {
                let mut map = serde_json::Map::new();
                map.insert("id".to_string(), serde_json::Value::String(r.id));
                map.insert("score".to_string(), serde_json::Value::from(r.score as f64));
                if let serde_json::Value::Object(m) = r.metadata {
                    for (k, v) in m {
                        map.insert(k, v);
                    }
                }
                serde_json::Value::Object(map)
            })
            .collect();

        apply_sort(&mut items, &query.sort);

        let total = items.len();
        let items = apply_pagination(items, &query.pagination);

        Ok(QueryResult {
            items,
            total,
            aggregations: None,
        })
    }

    async fn execute_hybrid(&self, query: &Query) -> Result<QueryResult, crate::Error> {
        let mut tantivy_items = self.execute_tantivy(query).await?.items;
        let mut lancedb_items = self.execute_lancedb(query).await?.items;

        // 使用简化 RRF (Reciprocal Rank Fusion) 合并结果
        let mut scores: HashMap<String, (serde_json::Value, f64)> = HashMap::new();
        let k = 60.0;

        for (rank, mut item) in tantivy_items.drain(..).enumerate() {
            let id = item
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let score = 1.0 / (k + rank as f64 + 1.0);
            if let Some((_, existing)) = scores.get_mut(&id) {
                *existing += score;
            } else {
                if let serde_json::Value::Object(ref mut map) = item {
                    map.remove("score");
                }
                scores.insert(id, (item, score));
            }
        }

        for (rank, mut item) in lancedb_items.drain(..).enumerate() {
            let id = item
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let score = 1.0 / (k + rank as f64 + 1.0);
            if let Some((_, existing)) = scores.get_mut(&id) {
                *existing += score;
            } else {
                if let serde_json::Value::Object(ref mut map) = item {
                    map.remove("score");
                }
                scores.insert(id, (item, score));
            }
        }

        let mut items: Vec<serde_json::Value> = scores
            .into_iter()
            .map(|(_, (mut item, score))| {
                if let serde_json::Value::Object(ref mut map) = item {
                    map.insert("score".to_string(), serde_json::Value::from(score));
                }
                item
            })
            .collect();

        items.sort_by(|a, b| {
            let a_score = a.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let b_score = b.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0);
            b_score.partial_cmp(&a_score).unwrap_or(Ordering::Equal)
        });

        apply_sort(&mut items, &query.sort);

        let total = items.len();
        let items = apply_pagination(items, &query.pagination);

        Ok(QueryResult {
            items,
            total,
            aggregations: None,
        })
    }
}

impl Default for QueryEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ==================== Helper Functions ====================

fn convert_to_storage_query(query: &Query) -> Result<StorageQuery, crate::Error> {
    let filters = convert_filter_to_storage_filters(&query.filter);
    let order_by = query
        .sort
        .first()
        .map(|s| format!("{} {}", s.field, format_direction(&s.direction)));

    let limit = query.pagination.as_ref().map(|p| p.limit);
    let offset = query.pagination.as_ref().map(|p| p.offset);

    Ok(StorageQuery {
        table: query.source.clone(),
        filters,
        order_by,
        limit,
        offset,
    })
}

fn format_direction(dir: &SortDirection) -> &'static str {
    match dir {
        SortDirection::Asc => "ASC",
        SortDirection::Desc => "DESC",
    }
}

fn convert_filter_to_storage_filters(filter: &Option<Filter>) -> Vec<StorageFilter> {
    let mut result = Vec::new();
    if let Some(f) = filter {
        collect_storage_filters(f, &mut result);
    }
    result
}

fn collect_storage_filters(filter: &Filter, result: &mut Vec<StorageFilter>) {
    match filter {
        Filter::And { filters } | Filter::Or { filters } => {
            for f in filters {
                collect_storage_filters(f, result);
            }
        }
        Filter::Not { filter } => {
            collect_storage_filters(filter, result);
        }
        Filter::Eq { field, value } => result.push(StorageFilter {
            field: field.clone(),
            op: "eq".to_string(),
            value: value.clone(),
        }),
        Filter::Ne { field, value } => result.push(StorageFilter {
            field: field.clone(),
            op: "ne".to_string(),
            value: value.clone(),
        }),
        Filter::Gt { field, value } => result.push(StorageFilter {
            field: field.clone(),
            op: "gt".to_string(),
            value: value.clone(),
        }),
        Filter::Gte { field, value } => result.push(StorageFilter {
            field: field.clone(),
            op: "gte".to_string(),
            value: value.clone(),
        }),
        Filter::Lt { field, value } => result.push(StorageFilter {
            field: field.clone(),
            op: "lt".to_string(),
            value: value.clone(),
        }),
        Filter::Lte { field, value } => result.push(StorageFilter {
            field: field.clone(),
            op: "lte".to_string(),
            value: value.clone(),
        }),
        Filter::In { field, values } => result.push(StorageFilter {
            field: field.clone(),
            op: "in".to_string(),
            value: serde_json::Value::Array(values.clone()),
        }),
        Filter::Contains { field, value } => result.push(StorageFilter {
            field: field.clone(),
            op: "contains".to_string(),
            value: serde_json::Value::String(value.clone()),
        }),
        Filter::StartsWith { field, value } => result.push(StorageFilter {
            field: field.clone(),
            op: "starts_with".to_string(),
            value: serde_json::Value::String(value.clone()),
        }),
        Filter::FullText { .. } | Filter::Vector { .. } => {}
    }
}

fn extract_fulltext_params(
    query: &Query,
) -> Result<(String, Option<Vec<String>>, usize), crate::Error> {
    let mut text_queries = Vec::new();
    let mut fields = None;
    let mut max_limit = 50;

    extract_fulltext_inner(query.filter.as_ref(), &mut text_queries, &mut fields)?;

    if text_queries.is_empty() {
        return Err(crate::Error::InvalidInput(
            "No fulltext query found in filter".to_string(),
        ));
    }

    let text_query = text_queries.join(" ");
    if let Some(ref p) = query.pagination {
        max_limit = p.offset + p.limit;
    }

    Ok((text_query, fields, max_limit))
}

fn extract_fulltext_inner(
    filter: Option<&Filter>,
    texts: &mut Vec<String>,
    fields: &mut Option<Vec<String>>,
) -> Result<(), crate::Error> {
    let Some(f) = filter else { return Ok(()) };
    match f {
        Filter::FullText { query, fields: f } => {
            texts.push(query.clone());
            if fields.is_none() && f.is_some() {
                *fields = f.clone();
            }
        }
        Filter::And { filters } | Filter::Or { filters } => {
            for sub in filters {
                extract_fulltext_inner(Some(sub), texts, fields)?;
            }
        }
        Filter::Not { filter } => extract_fulltext_inner(Some(filter), texts, fields)?,
        _ => {}
    }
    Ok(())
}

fn extract_vector_params(query: &Query) -> Result<(Vec<f32>, usize), crate::Error> {
    let mut vectors = Vec::new();
    let mut top_k = 10;

    extract_vector_inner(query.filter.as_ref(), &mut vectors, &mut top_k)?;

    if vectors.is_empty() {
        return Err(crate::Error::InvalidInput(
            "No vector query found in filter".to_string(),
        ));
    }

    Ok((vectors.into_iter().next().unwrap(), top_k))
}

fn extract_vector_inner(
    filter: Option<&Filter>,
    vectors: &mut Vec<Vec<f32>>,
    top_k: &mut usize,
) -> Result<(), crate::Error> {
    let Some(f) = filter else { return Ok(()) };
    match f {
        Filter::Vector { vector, top_k: k } => {
            vectors.push(vector.clone());
            *top_k = *k;
        }
        Filter::And { filters } | Filter::Or { filters } => {
            for sub in filters {
                extract_vector_inner(Some(sub), vectors, top_k)?;
            }
        }
        Filter::Not { filter } => extract_vector_inner(Some(filter), vectors, top_k)?,
        _ => {}
    }
    Ok(())
}

fn convert_filter_to_vector_filter(filter: &Option<Filter>) -> Option<VectorFilter> {
    let f = filter.as_ref()?;
    match f {
        Filter::Eq { field, value } => Some(VectorFilter {
            field: field.clone(),
            op: "eq".to_string(),
            value: value.clone(),
        }),
        Filter::And { filters } | Filter::Or { filters } => filters
            .iter()
            .find_map(|sub| convert_filter_to_vector_filter(&Some(sub.clone()))),
        _ => None,
    }
}

fn apply_pagination(
    items: Vec<serde_json::Value>,
    pagination: &Option<Pagination>,
) -> Vec<serde_json::Value> {
    match pagination {
        Some(p) => {
            let start = p.offset.min(items.len());
            let end = (p.offset + p.limit).min(items.len());
            items.into_iter().skip(start).take(end - start).collect()
        }
        None => items,
    }
}

fn apply_sort(items: &mut Vec<serde_json::Value>, sort: &[Sort]) {
    if sort.is_empty() {
        return;
    }
    items.sort_by(|a, b| {
        for s in sort {
            let a_val = a.get(&s.field);
            let b_val = b.get(&s.field);
            let ord = match (a_val, b_val) {
                (Some(av), Some(bv)) => compare_json_values(av, bv),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => continue,
            };
            let ord = match s.direction {
                SortDirection::Asc => ord,
                SortDirection::Desc => ord.reverse(),
            };
            if ord != Ordering::Equal {
                return ord;
            }
        }
        Ordering::Equal
    });
}

fn compare_json_values(a: &serde_json::Value, b: &serde_json::Value) -> Ordering {
    match (a, b) {
        (serde_json::Value::Number(an), serde_json::Value::Number(bn)) => {
            if let (Some(ai), Some(bi)) = (an.as_i64(), bn.as_i64()) {
                ai.cmp(&bi)
            } else if let (Some(af), Some(bf)) = (an.as_f64(), bn.as_f64()) {
                af.partial_cmp(&bf).unwrap_or(Ordering::Equal)
            } else {
                Ordering::Equal
            }
        }
        (serde_json::Value::String(as_), serde_json::Value::String(bs)) => as_.cmp(bs),
        (serde_json::Value::Bool(ab), serde_json::Value::Bool(bb)) => ab.cmp(bb),
        _ => Ordering::Equal,
    }
}

fn apply_projection(result: &mut QueryResult, projection: &[String]) {
    for item in &mut result.items {
        if let serde_json::Value::Object(ref mut map) = item {
            let keys: Vec<String> = map.keys().cloned().collect();
            for key in keys {
                if !projection.contains(&key) {
                    map.remove(&key);
                }
            }
        }
    }
}

fn agg_key(agg: &Aggregation) -> String {
    match agg {
        Aggregation::Count { alias, .. } => alias.clone().unwrap_or_else(|| "count".to_string()),
        Aggregation::Sum { alias, .. } => alias.clone().unwrap_or_else(|| "sum".to_string()),
        Aggregation::Avg { alias, .. } => alias.clone().unwrap_or_else(|| "avg".to_string()),
        Aggregation::Min { alias, .. } => alias.clone().unwrap_or_else(|| "min".to_string()),
        Aggregation::Max { alias, .. } => alias.clone().unwrap_or_else(|| "max".to_string()),
        Aggregation::GroupBy { field, .. } => format!("group_by_{}", field),
    }
}

fn execute_aggregation(
    items: &[serde_json::Value],
    agg: &Aggregation,
) -> Result<serde_json::Value, crate::Error> {
    match agg {
        Aggregation::Count { field, .. } => {
            if let Some(f) = field {
                let count = items.iter().filter(|item| item.get(f).is_some()).count();
                Ok(serde_json::json!(count))
            } else {
                Ok(serde_json::json!(items.len()))
            }
        }
        Aggregation::Sum { field, .. } => {
            let sum = items
                .iter()
                .filter_map(|item| item.get(field)?.as_f64())
                .sum::<f64>();
            Ok(serde_json::json!(sum))
        }
        Aggregation::Avg { field, .. } => {
            let values: Vec<f64> = items
                .iter()
                .filter_map(|item| item.get(field)?.as_f64())
                .collect();
            if values.is_empty() {
                Ok(serde_json::Value::Null)
            } else {
                let avg = values.iter().sum::<f64>() / values.len() as f64;
                Ok(serde_json::json!(avg))
            }
        }
        Aggregation::Min { field, .. } => {
            let min = items
                .iter()
                .filter_map(|item| item.get(field)?.as_f64())
                .min_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
            Ok(serde_json::json!(min))
        }
        Aggregation::Max { field, .. } => {
            let max = items
                .iter()
                .filter_map(|item| item.get(field)?.as_f64())
                .max_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
            Ok(serde_json::json!(max))
        }
        Aggregation::GroupBy { field, aggs } => {
            let mut groups: HashMap<String, Vec<serde_json::Value>> = HashMap::new();
            for item in items {
                if let Some(key) = item.get(field).and_then(|v| match v {
                    serde_json::Value::String(s) => Some(s.clone()),
                    serde_json::Value::Number(n) => Some(n.to_string()),
                    serde_json::Value::Bool(b) => Some(b.to_string()),
                    _ => None,
                }) {
                    groups.entry(key).or_default().push(item.clone());
                }
            }

            let mut group_result = serde_json::Map::new();
            for (key, group_items) in groups {
                let mut sub_results = serde_json::Map::new();
                for sub_agg in aggs {
                    let value = execute_aggregation(&group_items, sub_agg)?;
                    sub_results.insert(agg_key(sub_agg), value);
                }
                group_result.insert(key, serde_json::Value::Object(sub_results));
            }

            Ok(serde_json::Value::Object(group_result))
        }
    }
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_query() -> Query {
        Query {
            source: "notes".to_string(),
            filter: Some(Filter::Eq {
                field: "status".to_string(),
                value: serde_json::json!("active"),
            }),
            sort: vec![],
            pagination: None,
            aggregation: None,
            projection: None,
        }
    }

    #[test]
    fn test_query_dsl_serialization() {
        let query = Query {
            source: "notes".to_string(),
            filter: Some(Filter::And {
                filters: vec![
                    Filter::Eq {
                        field: "status".to_string(),
                        value: serde_json::json!("active"),
                    },
                    Filter::Contains {
                        field: "title".to_string(),
                        value: "hello".to_string(),
                    },
                ],
            }),
            sort: vec![Sort {
                field: "created_at".to_string(),
                direction: SortDirection::Desc,
            }],
            pagination: Some(Pagination {
                limit: 10,
                offset: 0,
            }),
            aggregation: None,
            projection: Some(vec!["id".to_string(), "title".to_string()]),
        };

        let json = serde_json::to_string_pretty(&query).unwrap();
        assert!(json.contains("\"op\": \"and\""));
        assert!(json.contains("\"direction\": \"desc\""));
    }

    #[test]
    fn test_optimizer_selects_sqlite_for_structured_query() {
        let query = make_query();
        let optimizer = QueryOptimizer::new();
        let plan = optimizer.optimize(&query);
        assert_eq!(plan.path, ExecutionPath::Sqlite);
    }

    #[test]
    fn test_optimizer_selects_tantivy_for_fulltext() {
        let query = Query {
            source: "notes".to_string(),
            filter: Some(Filter::FullText {
                query: "hello world".to_string(),
                fields: None,
            }),
            sort: vec![],
            pagination: None,
            aggregation: None,
            projection: None,
        };
        let optimizer = QueryOptimizer::new();
        let plan = optimizer.optimize(&query);
        assert_eq!(plan.path, ExecutionPath::Tantivy);
    }

    #[test]
    fn test_optimizer_selects_lancedb_for_vector() {
        let query = Query {
            source: "notes".to_string(),
            filter: Some(Filter::Vector {
                vector: vec![0.1, 0.2, 0.3],
                top_k: 5,
            }),
            sort: vec![],
            pagination: None,
            aggregation: None,
            projection: None,
        };
        let optimizer = QueryOptimizer::new();
        let plan = optimizer.optimize(&query);
        assert_eq!(plan.path, ExecutionPath::LanceDb);
    }

    #[test]
    fn test_optimizer_selects_hybrid_for_both() {
        let query = Query {
            source: "notes".to_string(),
            filter: Some(Filter::And {
                filters: vec![
                    Filter::FullText {
                        query: "hello".to_string(),
                        fields: None,
                    },
                    Filter::Vector {
                        vector: vec![0.1, 0.2],
                        top_k: 5,
                    },
                ],
            }),
            sort: vec![],
            pagination: None,
            aggregation: None,
            projection: None,
        };
        let optimizer = QueryOptimizer::new();
        let plan = optimizer.optimize(&query);
        assert_eq!(plan.path, ExecutionPath::Hybrid);
    }

    #[test]
    fn test_lru_cache_basic() {
        let mut cache = LruCache::new(2);
        cache.put("a".to_string(), 1);
        cache.put("b".to_string(), 2);
        assert_eq!(cache.get_mut(&"a".to_string()), Some(&mut 1));
        cache.put("c".to_string(), 3);
        assert_eq!(cache.get_mut(&"b".to_string()), None);
        assert_eq!(cache.get_mut(&"a".to_string()), Some(&mut 1));
        assert_eq!(cache.get_mut(&"c".to_string()), Some(&mut 3));
    }

    #[test]
    fn test_query_cache_ttl() {
        let cache = QueryCache::new(10);
        let result = QueryResult {
            items: vec![serde_json::json!({"id": "1"})],
            total: 1,
            aggregations: None,
        };
        cache.put(
            "key1".to_string(),
            "notes",
            result.clone(),
            Duration::from_secs(60),
        );
        assert!(cache.get("key1", "notes").is_some());
    }

    #[test]
    fn test_query_cache_expiration() {
        let cache = QueryCache::new(10);
        let result = QueryResult {
            items: vec![serde_json::json!({"id": "1"})],
            total: 1,
            aggregations: None,
        };
        cache.put("key1".to_string(), "notes", result, Duration::from_nanos(1));
        std::thread::sleep(Duration::from_millis(10));
        assert!(cache.get("key1", "notes").is_none());
    }

    #[test]
    fn test_adaptive_ttl() {
        let cache = QueryCache::new(10);
        // 高频变更
        for _ in 0..10 {
            cache.record_change("hot_source");
            std::thread::sleep(Duration::from_millis(10));
        }
        let ttl = cache.compute_ttl("hot_source");
        assert!(ttl <= Duration::from_secs(60));

        // 低频变更
        cache.record_change("cold_source");
        let ttl = cache.compute_ttl("cold_source");
        assert!(ttl >= Duration::from_secs(300));
    }

    #[test]
    fn test_pagination() {
        let items: Vec<serde_json::Value> = (0..10).map(|i| serde_json::json!({"id": i})).collect();
        let paginated = apply_pagination(
            items,
            &Some(Pagination {
                limit: 3,
                offset: 2,
            }),
        );
        assert_eq!(paginated.len(), 3);
        assert_eq!(paginated[0], serde_json::json!({"id": 2}));
    }

    #[test]
    fn test_sort() {
        let mut items = vec![
            serde_json::json!({"score": 3}),
            serde_json::json!({"score": 1}),
            serde_json::json!({"score": 2}),
        ];
        apply_sort(
            &mut items,
            &[Sort {
                field: "score".to_string(),
                direction: SortDirection::Asc,
            }],
        );
        assert_eq!(items[0], serde_json::json!({"score": 1}));
        assert_eq!(items[2], serde_json::json!({"score": 3}));
    }

    #[test]
    fn test_aggregation_count() {
        let items = vec![
            serde_json::json!({"status": "active"}),
            serde_json::json!({"status": "inactive"}),
            serde_json::json!({}),
        ];
        let agg = Aggregation::Count {
            field: Some("status".to_string()),
            alias: None,
        };
        let result = execute_aggregation(&items, &agg).unwrap();
        assert_eq!(result, serde_json::json!(2));
    }

    #[test]
    fn test_aggregation_sum_avg() {
        let items = vec![
            serde_json::json!({"value": 10.0}),
            serde_json::json!({"value": 20.0}),
            serde_json::json!({"value": 30.0}),
        ];
        let sum = execute_aggregation(
            &items,
            &Aggregation::Sum {
                field: "value".to_string(),
                alias: None,
            },
        )
        .unwrap();
        assert_eq!(sum, serde_json::json!(60.0));

        let avg = execute_aggregation(
            &items,
            &Aggregation::Avg {
                field: "value".to_string(),
                alias: None,
            },
        )
        .unwrap();
        assert_eq!(avg, serde_json::json!(20.0));
    }

    #[test]
    fn test_aggregation_group_by() {
        let items = vec![
            serde_json::json!({"category": "a", "value": 10.0}),
            serde_json::json!({"category": "a", "value": 20.0}),
            serde_json::json!({"category": "b", "value": 30.0}),
        ];
        let agg = Aggregation::GroupBy {
            field: "category".to_string(),
            aggs: vec![Aggregation::Sum {
                field: "value".to_string(),
                alias: Some("total".to_string()),
            }],
        };
        let result = execute_aggregation(&items, &agg).unwrap();
        let map = result.as_object().unwrap();
        assert!(map.contains_key("a"));
        assert!(map.contains_key("b"));
    }

    #[test]
    fn test_compare_json_values() {
        assert_eq!(
            compare_json_values(&serde_json::json!(1), &serde_json::json!(2)),
            Ordering::Less
        );
        assert_eq!(
            compare_json_values(&serde_json::json!("b"), &serde_json::json!("a")),
            Ordering::Greater
        );
    }

    #[test]
    fn test_projection() {
        let mut result = QueryResult {
            items: vec![serde_json::json!({
                "id": "1",
                "title": "Hello",
                "secret": "xyz"
            })],
            total: 1,
            aggregations: None,
        };
        apply_projection(&mut result, &["id".to_string(), "title".to_string()]);
        let item = result.items[0].as_object().unwrap();
        assert!(item.contains_key("id"));
        assert!(item.contains_key("title"));
        assert!(!item.contains_key("secret"));
    }

    #[test]
    fn test_cache_metrics() {
        let cache = QueryCache::new(5);
        let result = QueryResult {
            items: vec![],
            total: 0,
            aggregations: None,
        };
        cache.put(
            "k1".to_string(),
            "s1",
            result.clone(),
            Duration::from_secs(10),
        );
        cache.get("k1", "s1");
        cache.get("k2", "s1");
        let metrics = cache.stats();
        assert_eq!(metrics.hit_count, 1);
        assert_eq!(metrics.miss_count, 1);
        assert_eq!(metrics.size, 1);
    }

    #[test]
    fn test_vector_filter_extraction() {
        let filter = Some(Filter::And {
            filters: vec![
                Filter::Eq {
                    field: "status".to_string(),
                    value: serde_json::json!("active"),
                },
                Filter::Vector {
                    vector: vec![0.1, 0.2],
                    top_k: 10,
                },
            ],
        });
        let vfilter = convert_filter_to_vector_filter(&filter);
        assert!(vfilter.is_some());
        assert_eq!(vfilter.unwrap().field, "status");
    }

    #[test]
    fn test_fulltext_params_extraction() {
        let query = Query {
            source: "notes".to_string(),
            filter: Some(Filter::FullText {
                query: "hello world".to_string(),
                fields: Some(vec!["title".to_string()]),
            }),
            sort: vec![],
            pagination: Some(Pagination {
                limit: 5,
                offset: 0,
            }),
            aggregation: None,
            projection: None,
        };
        let (text, fields, limit) = extract_fulltext_params(&query).unwrap();
        assert_eq!(text, "hello world");
        assert_eq!(fields, Some(vec!["title".to_string()]));
        assert_eq!(limit, 5);
    }
}
