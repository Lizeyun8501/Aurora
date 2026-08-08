//! 核心事件定义

use serde::{Deserialize, Serialize};

/// 核心事件类型，覆盖所有模块间通信场景
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CoreEvent {
    /// 文档内容变更
    DocumentChanged {
        doc_id: String,
        change_summary: DocumentChangeSummary,
    },
    /// 同步进度更新
    SyncProgress { target_id: String, progress: f32 },
    /// 任务到期提醒
    TaskDue { task_id: String, due_time: u64 },
    /// AI 生成完成
    AIGenerationComplete { request_id: String, output: String },
    /// 权限变更
    PermissionChanged {
        resource_id: String,
        new_perms: PermissionSet,
    },
    /// 插件加载完成
    PluginLoaded { plugin_id: String },
    /// 块变更事件（内容编辑 → 知识网络）
    BlockChanged {
        doc_id: String,
        block_id: String,
        block_type: String,
        content: serde_json::Value,
    },
    /// 反向链接更新（知识网络 → 内容编辑）
    BacklinksUpdated { doc_id: String },
    /// 任务创建（GTD → 内容编辑）
    TaskCreated { task_id: String, title: String },
    /// 任务状态更新（内容编辑 → GTD）
    TaskUpdated { task_id: String, status: String },
    /// 素材添加
    AssetAdded {
        asset_hash: String,
        mime_type: String,
    },
    /// 索引重建请求
    IndexRebuildRequest { index_type: IndexType },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentChangeSummary {
    pub doc_id: String,
    pub changed_blocks: Vec<BlockChangeInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockChangeInfo {
    pub block_id: String,
    pub op_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionSet {
    pub resource_id: String,
    pub owner: String,
    pub permissions: Vec<PermissionEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionEntry {
    pub role: String,
    pub actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IndexType {
    FullText,
    Vector,
    Link,
}
