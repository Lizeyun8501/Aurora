//! 聚合 (Aggregate)
//!
//! 聚合通过回放事件序列重建状态。每个聚合维护自身版本号，
//! 每应用一个事件版本号自增。

use std::collections::HashMap;

use super::event::Event;

/// 聚合 trait，定义事件回放与状态读取的统一接口。
pub trait Aggregate {
    /// 应用一个事件到聚合，更新内部状态。
    fn apply_event(&mut self, event: &Event);
    /// 返回聚合当前状态的 JSON 快照。
    fn get_state(&self) -> serde_json::Value;
    /// 返回聚合当前版本号。
    fn get_version(&self) -> u64;
}

/// 文档聚合，维护文档内所有块的状态。
#[derive(Debug, Clone)]
pub struct DocumentAggregate {
    /// 文档 ID
    pub doc_id: String,
    /// 文档内的块状态，键为 block_id
    pub blocks: HashMap<String, serde_json::Value>,
    /// 当前版本号
    pub version: u64,
}

impl DocumentAggregate {
    /// 创建一个新的文档聚合。
    pub fn new(doc_id: &str) -> Self {
        Self {
            doc_id: doc_id.to_string(),
            blocks: HashMap::new(),
            version: 0,
        }
    }
}

impl Aggregate for DocumentAggregate {
    fn apply_event(&mut self, event: &Event) {
        match event.op_type.as_str() {
            "create" | "update" => {
                self.blocks
                    .insert(event.block_id.clone(), event.payload.clone());
            }
            "delete" => {
                self.blocks.remove(&event.block_id);
            }
            _ => {}
        }
        self.version += 1;
    }

    fn get_state(&self) -> serde_json::Value {
        serde_json::json!({
            "doc_id": self.doc_id,
            "blocks": self.blocks,
            "version": self.version,
        })
    }

    fn get_version(&self) -> u64 {
        self.version
    }
}

/// 块聚合，维护单个块的内容与属性。
#[derive(Debug, Clone)]
pub struct BlockAggregate {
    /// 块 ID
    pub block_id: String,
    /// 块内容
    pub content: serde_json::Value,
    /// 块属性
    pub properties: serde_json::Value,
    /// 当前版本号
    pub version: u64,
}

impl BlockAggregate {
    /// 创建一个新的块聚合。
    pub fn new(block_id: &str) -> Self {
        Self {
            block_id: block_id.to_string(),
            content: serde_json::Value::Null,
            properties: serde_json::Value::Object(serde_json::Map::new()),
            version: 0,
        }
    }
}

impl Aggregate for BlockAggregate {
    fn apply_event(&mut self, event: &Event) {
        match event.op_type.as_str() {
            "create" | "update" => {
                self.content = event.payload.clone();
            }
            "property_change" => {
                self.properties = event.payload.clone();
            }
            "delete" => {
                self.content = serde_json::Value::Null;
            }
            _ => {}
        }
        self.version += 1;
    }

    fn get_state(&self) -> serde_json::Value {
        serde_json::json!({
            "block_id": self.block_id,
            "content": self.content,
            "properties": self.properties,
            "version": self.version,
        })
    }

    fn get_version(&self) -> u64 {
        self.version
    }
}

/// 工作空间聚合，维护工作空间下的文档列表。
#[derive(Debug, Clone)]
pub struct WorkspaceAggregate {
    /// 工作空间 ID
    pub workspace_id: String,
    /// 工作空间下的文档 ID 列表
    pub documents: Vec<String>,
    /// 当前版本号
    pub version: u64,
}

impl WorkspaceAggregate {
    /// 创建一个新的工作空间聚合。
    pub fn new(workspace_id: &str) -> Self {
        Self {
            workspace_id: workspace_id.to_string(),
            documents: Vec::new(),
            version: 0,
        }
    }
}

impl Aggregate for WorkspaceAggregate {
    fn apply_event(&mut self, event: &Event) {
        match event.op_type.as_str() {
            "create" => {
                if !self.documents.contains(&event.block_id) {
                    self.documents.push(event.block_id.clone());
                }
            }
            "delete" => {
                self.documents.retain(|d| d != &event.block_id);
            }
            _ => {}
        }
        self.version += 1;
    }

    fn get_state(&self) -> serde_json::Value {
        serde_json::json!({
            "workspace_id": self.workspace_id,
            "documents": self.documents,
            "version": self.version,
        })
    }

    fn get_version(&self) -> u64 {
        self.version
    }
}
