//! MockL1 — shared test utilities for aurora-core integration tests.
//!
//! Provides in-memory mocks of L1 infrastructure traits (Storage), deterministic
//! data generators for blocks/documents, and a mock sync bus used to simulate
//! document convergence between two peers without a real network.
//!
//! This module is shared across the Task 6.3 test suite (`property_tests`,
//! `crdt_consistency`, `e2e_flows`, `perf_baseline`). It is intentionally
//! dependency-light: it relies only on types already exported by `aurora_core`
//! plus `proptest` strategies for generation.

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use aurora_core::l3_domain::content_editor::{Block, BlockType, Document};
use aurora_core::traits::storage::{QueryFilter, Record, Storage, StorageOp, StorageQuery};

// ==================== MockStorage (L1 storage mock) ====================

/// In-memory implementation of the L1 `Storage` trait.
///
/// Backed by a `HashMap<String, Vec<u8>>` behind a `Mutex` so it is `Send + Sync`.
/// `query` performs a trivial linear scan over a `__records__` namespace so that
/// tests exercising the query path have a working fixture without a real SQLite DB.
pub struct MockStorage {
    inner: Mutex<HashMap<String, Vec<u8>>>,
}

impl MockStorage {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }

    /// Insert a JSON record into the `__records__` namespace (used by `query`).
    pub fn insert_record(&self, key: &str, value: serde_json::Value) {
        let mut map = self.inner.lock().unwrap();
        let k = format!("__records__:{key}");
        let bytes = serde_json::to_vec(&value).unwrap_or_default();
        map.insert(k, bytes);
    }

    /// Count of all keys (any namespace).
    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.lock().unwrap().is_empty()
    }
}

impl Default for MockStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Storage for MockStorage {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, aurora_core::Error> {
        Ok(self.inner.lock().unwrap().get(key).cloned())
    }

    async fn put(&self, key: &str, value: &[u8]) -> Result<(), aurora_core::Error> {
        self.inner
            .lock()
            .unwrap()
            .insert(key.to_string(), value.to_vec());
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), aurora_core::Error> {
        self.inner.lock().unwrap().remove(key);
        Ok(())
    }

    async fn query(&self, q: &StorageQuery) -> Result<Vec<Record>, aurora_core::Error> {
        let map = self.inner.lock().unwrap();
        let mut records: Vec<Record> = Vec::new();
        for (k, v) in map.iter() {
            if !k.starts_with("__records__:") {
                continue;
            }
            let value: serde_json::Value =
                serde_json::from_slice(v).unwrap_or(serde_json::Value::Null);
            if record_matches(&value, &q.filters) {
                records.push(Record { data: value });
            }
        }
        // Apply limit/offset (deterministic order by serialized form for stability).
        records.sort_by(|a, b| {
            serde_json::to_string(&a.data)
                .unwrap_or_default()
                .cmp(&serde_json::to_string(&b.data).unwrap_or_default())
        });
        if let Some(offset) = q.offset {
            if offset >= records.len() {
                records.clear();
            } else {
                records.drain(0..offset);
            }
        }
        if let Some(limit) = q.limit {
            records.truncate(limit);
        }
        Ok(records)
    }

    async fn transaction(&self, ops: &[StorageOp]) -> Result<(), aurora_core::Error> {
        let mut map = self.inner.lock().unwrap();
        for op in ops {
            match op {
                StorageOp::Put { key, value } => {
                    map.insert(key.clone(), value.clone());
                }
                StorageOp::Delete { key } => {
                    map.remove(key);
                }
            }
        }
        Ok(())
    }
}

/// Evaluate a trivial set of equality-style filters against a JSON record.
fn record_matches(value: &serde_json::Value, filters: &[QueryFilter]) -> bool {
    for f in filters {
        let field_val = value.get(&f.field);
        let matches = match f.op.as_str() {
            "eq" => field_val.map(|v| v == &f.value).unwrap_or(false),
            "ne" => field_val.map(|v| v != &f.value).unwrap_or(true),
            "gt" => field_val
                .and_then(|v| v.as_f64())
                .zip(f.value.as_f64())
                .map(|(a, b)| a > b)
                .unwrap_or(false),
            "lt" => field_val
                .and_then(|v| v.as_f64())
                .zip(f.value.as_f64())
                .map(|(a, b)| a < b)
                .unwrap_or(false),
            "contains" => field_val
                .and_then(|v| v.as_str())
                .map(|s| s.contains(f.value.as_str().unwrap_or("")))
                .unwrap_or(false),
            _ => true,
        };
        if !matches {
            return false;
        }
    }
    true
}

// ==================== Data generators ====================

/// Build a text block with the given content. The block id is a fresh UUID.
pub fn make_text_block(content: &str) -> Block {
    Block::text(content.to_string())
}

/// Build a heading block of the given level (clamped to 1..=6 by `Block::heading`).
pub fn make_heading_block(level: u8, content: &str) -> Block {
    Block::heading(level, content.to_string())
}

/// Build a document titled `title` containing the provided blocks.
pub fn make_document(title: &str, blocks: Vec<Block>) -> Document {
    let mut doc = Document::new(title.to_string());
    for b in blocks {
        doc = doc.with_block(b);
    }
    doc
}

/// Convenience: classify a `BlockType` into a short stable tag used by generators.
pub fn block_type_tag(bt: &BlockType) -> &'static str {
    match bt {
        BlockType::Text => "text",
        BlockType::Heading => "heading",
        BlockType::Code => "code",
        BlockType::Image => "image",
        BlockType::Table => "table",
        BlockType::Divider => "divider",
        BlockType::Quote => "quote",
        BlockType::ListItem => "list_item",
        BlockType::TodoItem => "todo_item",
        BlockType::Custom(_) => "custom",
    }
}

// ==================== MockSyncBus ====================

/// A minimal mock sync channel that exchanges serialized document snapshots
/// between two "peers" without a real network. Used by E2E sync-flow tests to
/// assert two replicas converge after exchanging state.
pub struct MockSyncBus {
    inbox: Arc<Mutex<Vec<Vec<u8>>>>,
}

impl MockSyncBus {
    pub fn new() -> Self {
        Self {
            inbox: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Push a serialized snapshot onto the bus.
    pub fn publish(&self, snapshot: Vec<u8>) {
        self.inbox.lock().unwrap().push(snapshot);
    }

    /// Drain all snapshots currently on the bus (FIFO order).
    pub fn drain(&self) -> Vec<Vec<u8>> {
        std::mem::take(&mut *self.inbox.lock().unwrap())
    }

    /// Round-trip a document through JSON serialization (simulates save/reload).
    pub fn serialize_doc(doc: &Document) -> Vec<u8> {
        serde_json::to_vec(doc).expect("doc must serialize")
    }

    /// Restore a document from a JSON snapshot (simulates reload).
    pub fn deserialize_doc(bytes: &[u8]) -> Document {
        serde_json::from_slice(bytes).expect("doc must deserialize")
    }
}

impl Default for MockSyncBus {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience re-exports so test files can `use common::prelude::*;`.
pub mod prelude {
    pub use super::{make_document, make_heading_block, make_text_block, MockStorage, MockSyncBus};
}
