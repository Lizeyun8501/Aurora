//! 事件定义
//!
//! 事件是事件溯源引擎中不可变的最小操作单元，所有用户操作都被
//! 记录为事件序列，通过回放事件即可重建聚合状态。

use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// 操作类型枚举，覆盖块级与文档级变更语义。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OpType {
    /// 创建
    Create,
    /// 更新
    Update,
    /// 删除
    Delete,
    /// 移动
    Move,
    /// 属性变更
    PropertyChange,
    /// 合并
    Merge,
}

impl OpType {
    /// 返回操作类型的字符串表示，用于持久化存储。
    pub fn as_str(&self) -> &'static str {
        match self {
            OpType::Create => "create",
            OpType::Update => "update",
            OpType::Delete => "delete",
            OpType::Move => "move",
            OpType::PropertyChange => "property_change",
            OpType::Merge => "merge",
        }
    }
}

/// 事件，事件溯源引擎中不可变的操作记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    /// 事件唯一 ID
    pub event_id: String,
    /// 关联的块 ID
    pub block_id: String,
    /// 操作类型
    pub op_type: String,
    /// 操作负载
    pub payload: serde_json::Value,
    /// 事件时间戳 (毫秒)
    pub timestamp: u64,
    /// 操作发起用户 ID
    pub user_id: String,
    /// 操作发起设备 ID
    pub device_id: String,
    /// 可选的签名数据
    pub signature: Option<Vec<u8>>,
}

impl Event {
    /// 创建一个新事件，自动生成 `event_id` 与 `timestamp`。
    pub fn new(
        block_id: &str,
        op_type: OpType,
        payload: serde_json::Value,
        user_id: &str,
        device_id: &str,
    ) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        Self {
            event_id: uuid::Uuid::new_v4().to_string(),
            block_id: block_id.to_string(),
            op_type: op_type.as_str().to_string(),
            payload,
            timestamp,
            user_id: user_id.to_string(),
            device_id: device_id.to_string(),
            signature: None,
        }
    }
}
