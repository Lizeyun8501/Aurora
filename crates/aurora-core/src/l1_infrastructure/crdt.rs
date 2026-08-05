//! CRDT 引擎 (基于 Loro)
//!
//! 提供无冲突复制数据类型 (CRDT) 能力，支持多端协同编辑与离线合并。
//! 底层使用 [Loro](https://loro.dev) 实现，确保协议长期稳定。

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Mutex;

use crate::traits::crdt_engine::{ChangeSummary, CrdtEngine, Event, MergeResult, Timestamp};

/// Loro CRDT 文档类型再导出。
///
/// 供 `traits::CrdtEngine` Trait 的 `create_document` 返回值使用。
/// `loro` 已作为 workspace 依赖加入，此处直接再导出真实类型。
pub use loro::LoroDoc;

/// 基于 Loro 的 CRDT 引擎实现。
pub struct LoroCrdtEngine {
    docs: Mutex<HashMap<String, LoroDoc>>,
}

impl LoroCrdtEngine {
    /// 创建新的 Loro CRDT 引擎实例。
    pub fn new() -> Self {
        Self {
            docs: Mutex::new(HashMap::new()),
        }
    }
}

impl Default for LoroCrdtEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CrdtEngine for LoroCrdtEngine {
    async fn create_document(&self, doc_id: &str) -> Result<LoroDoc, crate::Error> {
        let mut docs = self
            .docs
            .lock()
            .map_err(|_| crate::Error::Internal("CRDT doc mutex poisoned".to_string()))?;
        let doc = LoroDoc::new();
        docs.insert(doc_id.to_string(), doc.clone());
        Ok(doc)
    }

    async fn apply_ops(&self, doc_id: &str, ops: &[u8]) -> Result<ChangeSummary, crate::Error> {
        let mut docs = self
            .docs
            .lock()
            .map_err(|_| crate::Error::Internal("CRDT doc mutex poisoned".to_string()))?;
        let doc = docs
            .get_mut(doc_id)
            .ok_or_else(|| crate::Error::NotFound(format!("doc not found: {}", doc_id)))?;
        doc.import(ops)
            .map_err(|e| crate::Error::Internal(format!("loro import failed: {}", e)))?;
        Ok(ChangeSummary {
            doc_id: doc_id.to_string(),
            changes: vec![],
        })
    }

    async fn get_snapshot(&self, doc_id: &str) -> Result<Vec<u8>, crate::Error> {
        let docs = self
            .docs
            .lock()
            .map_err(|_| crate::Error::Internal("CRDT doc mutex poisoned".to_string()))?;
        let doc = docs
            .get(doc_id)
            .ok_or_else(|| crate::Error::NotFound(format!("doc not found: {}", doc_id)))?;
        let bytes = doc.export(loro::ExportMode::Snapshot).map_err(|e| crate::Error::Internal(format!("loro export failed: {:?}", e)))?;
        Ok(bytes)
    }

    fn get_history(&self, doc_id: &str, _since: Option<Timestamp>) -> Vec<Event> {
        match self.docs.lock() {
            Ok(docs) => {
                if !docs.contains_key(doc_id) {
                    tracing::warn!("get_history called for unknown doc: {}", doc_id);
                }
            }
            Err(_) => tracing::warn!("CRDT doc mutex poisoned"),
        }
        // Loro 的事件历史需要通过订阅回调收集，此处返回空列表作为占位。
        vec![]
    }

    async fn merge_branch(&self, doc_id: &str, _branch_id: &str) -> Result<MergeResult, crate::Error> {
        let docs = self
            .docs
            .lock()
            .map_err(|_| crate::Error::Internal("CRDT doc mutex poisoned".to_string()))?;
        if !docs.contains_key(doc_id) {
            return Err(crate::Error::NotFound(format!("doc not found: {}", doc_id)));
        }
        // Loro 的合并通过 import 自动完成冲突解决，此处返回无冲突结果。
        Ok(MergeResult {
            conflicts: vec![],
            merged_blocks: vec![],
        })
    }
}
