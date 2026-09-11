//! 核心事件定义 — V26 DK-00 契约冻结（41 类事件字典）
//!
//! 沿用远端 PascalCase 命名与 Rust↔TS 严格镜像实践
//! （本文件 ↔ `shared/types/src/events.ts`）。★ 标记的 12 类为原有变体，
//! V26 扩展至 41 类而非另起炉灶（V26-0 §4.1 表 4-1）。
//!
//! 通道与持久化规则（V26 硬约束）：
//! - `High`：UI 实时通道，不持久化（出队即焚）；积压不延迟
//! - `Medium`：必须持久化 + 跨通道有序（排序键 `channel_rank, seq`）
//! - `Low`：持久化；Medium 积压 >50 条时 Low 延迟 5s
//!
//! 顺序保证作用域：`seq` 排序以 `aggregate_id` 为作用域（同一笔记严格有序，
//! 不同笔记可并行处理），不承诺全局严格有序。

use serde::{Deserialize, Serialize};

/// 事件通道（决定持久化与延迟策略）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EventChannel {
    /// UI 实时通道 — 不持久化。
    High,
    /// 有序持久化通道（默认）。
    Medium,
    /// 后台通道 — 持久化；Medium 积压 >50 条时延迟 5s。
    Low,
}

/// 事件生产者（V26 §4.1 Actor 模型）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Actor {
    /// 用户操作。
    User,
    /// Agent 会话（带会话 ID）。
    Agent { session_id: String },
    /// 系统内部（如投影重建、FSRS 调度）。
    System,
    /// 来自同步（带来源设备 ID）。
    Sync { device_id: String },
}

/// 事件通用信封（V26 §4.1）— 持久化层使用的完整包装。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope {
    /// 幂等键（UUIDv7）。
    pub event_id: String,
    /// 判别名，如 `"NoteCreated"`（与 [`CoreEvent`] 变体名一致）。
    pub event_type: String,
    /// 聚合根 ID（笔记 ID / 任务 ID）；seq 排序作用域。
    pub aggregate_id: String,
    /// 通道。
    pub channel: EventChannel,
    /// 全局单调序号（作用域：aggregate_id）。
    pub seq: u64,
    /// 生产者。
    pub actor: Actor,
    /// 毫秒时间戳。
    pub occurred_at: i64,
    /// payload（`CoreEvent` 的 serde 序列化）。
    pub payload: serde_json::Value,
    /// payload 结构版本。
    pub schema_ver: u16,
}

impl CoreEvent {
    /// 契约元数据：`(变体名, 通道, 是否持久化)` — V26 表 4-1 的机器可读形态。
    ///
    /// 事件字典冻结后新增变体必须同步更新此表与 `shared/types/src/events.ts`，
    /// 并走 ADR（契约治理见 `docs/adr/ADR-002-contract-governance.md`）。
    pub const DICT: &'static [(&'static str, EventChannel, bool)] = &[
        ("DocumentChanged", EventChannel::Medium, true),
        ("BlockChanged", EventChannel::Medium, true),
        ("NoteCreated", EventChannel::Medium, true),
        ("NoteDeleted", EventChannel::Medium, true),
        ("NoteRestored", EventChannel::Medium, true),
        ("NoteMoved", EventChannel::Medium, true),
        ("NoteTitleChanged", EventChannel::Medium, true),
        ("BacklinksUpdated", EventChannel::Medium, true),
        ("LinkCreated", EventChannel::Medium, true),
        ("LinkRemoved", EventChannel::Medium, true),
        ("TaskCreated", EventChannel::Medium, true),
        ("TaskUpdated", EventChannel::Medium, true),
        ("TaskCompleted", EventChannel::Medium, true),
        ("TaskDependencyChanged", EventChannel::Medium, true),
        ("TaskDue", EventChannel::Low, true),
        ("TaskTimerStarted", EventChannel::High, false),
        ("TaskTimerStopped", EventChannel::Medium, true),
        ("HabitChecked", EventChannel::Medium, true),
        ("ReviewCardDue", EventChannel::Low, true),
        ("ReviewCardRated", EventChannel::Medium, true),
        ("SyncProgress", EventChannel::High, false),
        ("SyncCompleted", EventChannel::Low, true),
        ("SyncFailed", EventChannel::Medium, true),
        ("SyncDegraded", EventChannel::Medium, true),
        ("SyncConflictDetected", EventChannel::Medium, true),
        ("ConflictResolved", EventChannel::Medium, true),
        ("DevicePaired", EventChannel::Medium, true),
        ("DeviceRevoked", EventChannel::Medium, true),
        ("AIGenerationComplete", EventChannel::Medium, true),
        ("AILiquifyProposed", EventChannel::High, false),
        ("AIProposalCommitted", EventChannel::Medium, true),
        ("AgentSessionStarted", EventChannel::Medium, true),
        ("AgentToolInvoked", EventChannel::Medium, true),
        ("AgentSessionExpired", EventChannel::Low, true),
        ("AgentKilled", EventChannel::High, true),
        ("PermissionChanged", EventChannel::Medium, true),
        ("PluginLoaded", EventChannel::Low, true),
        ("PluginUnloaded", EventChannel::Low, true),
        ("PluginPermissionDenied", EventChannel::Medium, true),
        ("AssetAdded", EventChannel::Medium, true),
        ("IndexRebuildRequest", EventChannel::Low, true),
    ];

    /// 事件判别名（信封 `event_type` 字段的来源）。
    pub fn event_type(&self) -> &'static str {
        match self {
            CoreEvent::DocumentChanged { .. } => "DocumentChanged",
            CoreEvent::BlockChanged { .. } => "BlockChanged",
            CoreEvent::NoteCreated { .. } => "NoteCreated",
            CoreEvent::NoteDeleted { .. } => "NoteDeleted",
            CoreEvent::NoteRestored { .. } => "NoteRestored",
            CoreEvent::NoteMoved { .. } => "NoteMoved",
            CoreEvent::NoteTitleChanged { .. } => "NoteTitleChanged",
            CoreEvent::BacklinksUpdated { .. } => "BacklinksUpdated",
            CoreEvent::LinkCreated { .. } => "LinkCreated",
            CoreEvent::LinkRemoved { .. } => "LinkRemoved",
            CoreEvent::TaskCreated { .. } => "TaskCreated",
            CoreEvent::TaskUpdated { .. } => "TaskUpdated",
            CoreEvent::TaskCompleted { .. } => "TaskCompleted",
            CoreEvent::TaskDependencyChanged { .. } => "TaskDependencyChanged",
            CoreEvent::TaskDue { .. } => "TaskDue",
            CoreEvent::TaskTimerStarted { .. } => "TaskTimerStarted",
            CoreEvent::TaskTimerStopped { .. } => "TaskTimerStopped",
            CoreEvent::HabitChecked { .. } => "HabitChecked",
            CoreEvent::ReviewCardDue { .. } => "ReviewCardDue",
            CoreEvent::ReviewCardRated { .. } => "ReviewCardRated",
            CoreEvent::SyncProgress { .. } => "SyncProgress",
            CoreEvent::SyncCompleted { .. } => "SyncCompleted",
            CoreEvent::SyncFailed { .. } => "SyncFailed",
            CoreEvent::SyncDegraded { .. } => "SyncDegraded",
            CoreEvent::SyncConflictDetected { .. } => "SyncConflictDetected",
            CoreEvent::ConflictResolved { .. } => "ConflictResolved",
            CoreEvent::DevicePaired { .. } => "DevicePaired",
            CoreEvent::DeviceRevoked { .. } => "DeviceRevoked",
            CoreEvent::AIGenerationComplete { .. } => "AIGenerationComplete",
            CoreEvent::AILiquifyProposed { .. } => "AILiquifyProposed",
            CoreEvent::AIProposalCommitted { .. } => "AIProposalCommitted",
            CoreEvent::AgentSessionStarted { .. } => "AgentSessionStarted",
            CoreEvent::AgentToolInvoked { .. } => "AgentToolInvoked",
            CoreEvent::AgentSessionExpired { .. } => "AgentSessionExpired",
            CoreEvent::AgentKilled { .. } => "AgentKilled",
            CoreEvent::PermissionChanged { .. } => "PermissionChanged",
            CoreEvent::PluginLoaded { .. } => "PluginLoaded",
            CoreEvent::PluginUnloaded { .. } => "PluginUnloaded",
            CoreEvent::PluginPermissionDenied { .. } => "PluginPermissionDenied",
            CoreEvent::AssetAdded { .. } => "AssetAdded",
            CoreEvent::IndexRebuildRequest { .. } => "IndexRebuildRequest",
        }
    }

    /// 事件通道（契约元数据查询）。
    pub fn channel(&self) -> EventChannel {
        Self::DICT
            .iter()
            .find(|(n, _, _)| *n == self.event_type())
            .map(|(_, ch, _)| *ch)
            .expect("CoreEvent variant missing from DICT")
    }

    /// 是否持久化（契约元数据查询）。
    pub fn is_persistent(&self) -> bool {
        Self::DICT
            .iter()
            .find(|(n, _, _)| *n == self.event_type())
            .map(|(_, _, p)| *p)
            .expect("CoreEvent variant missing from DICT")
    }
}

/// 核心事件类型，覆盖所有模块间通信场景（41 类，V26 表 4-1 冻结）
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
    /// 任务创建（GTD → 内容编辑）。V26: 关联来源笔记/块。
    TaskCreated {
        task_id: String,
        title: String,
        note_id: Option<String>,
        block_id: Option<String>,
    },
    /// 任务状态更新（内容编辑 → GTD）。V26: 携带变更字段清单。
    TaskUpdated {
        task_id: String,
        status: String,
        changed_fields: Vec<String>,
    },
    /// 素材添加（BlobStore → 搜索/资产库）。V26: 加 size。
    AssetAdded {
        asset_hash: String,
        mime_type: String,
        size: u64,
    },
    /// 索引重建请求。V26: 加 scope（全量/增量）。
    IndexRebuildRequest {
        index_type: IndexType,
        scope: IndexRebuildScope,
    },
    // ===== 以下为 V26 新增（29 类）=====
    /// 笔记创建（WritePath → 搜索/列表/图谱）。
    NoteCreated {
        note_id: String,
        workspace_id: String,
        title: String,
    },
    /// 笔记删除（WritePath → 回收站/搜索）。
    NoteDeleted {
        note_id: String,
        origin_path: String,
    },
    /// 笔记从回收站恢复（TrashService → 搜索/列表）。
    NoteRestored {
        note_id: String,
        target_parent_id: String,
    },
    /// 笔记移动（WritePath → 列表/图谱）。
    NoteMoved {
        note_id: String,
        from_parent: String,
        to_parent: String,
    },
    /// 笔记标题变更（WritePath → 双链/搜索/列表）。
    NoteTitleChanged {
        note_id: String,
        old_title: String,
        new_title: String,
    },
    /// 双向链接创建（BidiLink → 图谱/搜索）。
    LinkCreated {
        source_id: String,
        target_id: String,
        link_type: String,
    },
    /// 双向链接移除（BidiLink → 图谱）。
    LinkRemoved {
        source_id: String,
        target_id: String,
    },
    /// 任务完成（TaskEngine → 统计/依赖解锁）。
    TaskCompleted {
        task_id: String,
        completed_at: i64,
        actual_minutes: Option<u32>,
    },
    /// 任务依赖变更（TaskEngine → TodayView）。环检测由 TaskEngine 端口负责。
    TaskDependencyChanged {
        task_id: String,
        depends_on: String,
        op: DependencyOp,
    },
    /// 番茄钟启动（High 通道，不持久化）。
    TaskTimerStarted { task_id: String, started_at: i64 },
    /// 番茄钟停止（TaskEngine → 任务/统计）。
    TaskTimerStopped {
        task_id: String,
        elapsed_minutes: u32,
    },
    /// 习惯打卡（TaskEngine → 统计）。
    HabitChecked {
        habit_id: String,
        date: String,
        streak: u32,
    },
    /// 复习卡到期（FSRS → Distill/通知）。
    ReviewCardDue {
        card_id: String,
        note_id: String,
        scheduled_at: i64,
    },
    /// 复习卡评分（FSRS → 统计/调度）。
    ReviewCardRated {
        card_id: String,
        rating: Rating,
        next_due: i64,
    },
    /// 同步完成（SyncRouter → 状态栏/审计）。
    SyncCompleted {
        target_id: String,
        docs_synced: u32,
        bytes: u64,
    },
    /// 同步失败（SyncRouter → 状态栏/告警）。error_code 对应 62 错误码 B 类。
    SyncFailed {
        target_id: String,
        error_code: String,
        retryable: bool,
    },
    /// 同步降级（SyncRouter → 状态栏）：链路切换。
    SyncDegraded {
        from_target: String,
        to_target: String,
        reason: String,
    },
    /// 同步冲突检出（SyncRouter → 冲突 UI）。
    SyncConflictDetected {
        doc_id: String,
        local_v: u64,
        remote_v: u64,
    },
    /// 冲突解决（冲突 UI → 写模型）。
    ConflictResolved {
        doc_id: String,
        strategy: ConflictStrategy,
        kept: String,
    },
    /// 设备配对完成（DeviceService → 同步/审计）。
    DevicePaired {
        device_id: String,
        pubkey: String,
        alias: String,
    },
    /// 设备吊销（DeviceService → 同步/审计）。
    DeviceRevoked {
        device_id: String,
        revoked_by: String,
    },
    /// AI 液化提案生成（High 通道，不持久化）。
    /// 硬约束：AI 结果不得直接落库 — 本事件只产生提案。
    AILiquifyProposed {
        request_id: String,
        proposals: Vec<serde_json::Value>,
    },
    /// AI 提案提交（UI 确认 → 写模型）。用户勾选后由本事件触发正规写入（铁律 5）。
    AIProposalCommitted {
        request_id: String,
        accepted_ids: Vec<String>,
    },
    /// Agent 会话启动（AgentGateway → 审计/UI）。
    AgentSessionStarted {
        session_id: String,
        agent_id: String,
        permissions: Vec<String>,
    },
    /// Agent 工具调用（AgentGateway → 审计）。
    AgentToolInvoked {
        session_id: String,
        tool: String,
        params: serde_json::Value,
        result: String,
    },
    /// Agent 会话过期（AgentGateway → 审计/UI）。
    AgentSessionExpired { session_id: String, expired_at: i64 },
    /// Agent 终止（Kill-Switch → 审计）。
    AgentKilled { session_id: String, reason: String },
    /// 插件卸载（PluginRuntime → UI）。
    PluginUnloaded { plugin_id: String, reason: String },
    /// 插件权限拒绝（PluginRuntime → UI/审计）。
    PluginPermissionDenied {
        plugin_id: String,
        capability: String,
    },
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

/// 索引重建范围（V26 表 4-1 #41）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndexRebuildScope {
    /// 全量重建。
    Full,
    /// 仅指定文档。
    Docs(Vec<String>),
}

/// 任务依赖操作（V26 表 4-1 #14）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyOp {
    /// 添加依赖（TaskEngine 端口做环检测）。
    Add,
    /// 移除依赖。
    Remove,
}

/// 复习评分（FSRS 四档，V26 表 4-1 #20）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rating {
    Again,
    Hard,
    Good,
    Easy,
}

/// 冲突解决策略（V26 表 4-1 #26）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictStrategy {
    /// 保留本地。
    KeepLocal,
    /// 保留远端。
    KeepRemote,
    /// 保留两份（冲突副本）。
    KeepBoth,
}
