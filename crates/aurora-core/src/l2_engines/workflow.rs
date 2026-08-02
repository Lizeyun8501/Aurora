//! 工作流引擎 (Workflow Engine)
//!
//! 基于状态机的异步工作流引擎，支持：
//! - 状态机 DSL（以 serde_json 描述状态节点与迁移条件）
//! - 三种触发器：时间触发、事件触发、API 触发
//! - 异步任务执行器（tokio::sync::mpsc），内置重试与死信队列

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::Error;

// ---------------------------------------------------------------------------
// 状态机 DSL
// ---------------------------------------------------------------------------

/// 工作流定义，通过 serde_json 反序列化构建状态机。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkflowDefinition {
    /// 工作流唯一标识
    pub id: String,
    /// 工作流名称
    pub name: String,
    /// 状态节点列表
    pub states: Vec<StateNode>,
    /// 状态迁移列表
    pub transitions: Vec<Transition>,
    /// 变量定义（用于运行时上下文）
    #[serde(default)]
    pub variables: HashMap<String, serde_json::Value>,
}

impl WorkflowDefinition {
    /// 从 JSON 字符串解析工作流定义。
    pub fn from_json(json: &str) -> Result<Self, Error> {
        serde_json::from_str(json).map_err(Error::Serialization)
    }

    /// 获取初始状态节点。
    pub fn initial_state(&self) -> Option<&StateNode> {
        self.states.iter().find(|s| s.state_type == StateType::Initial)
    }

    /// 根据状态 ID 查找节点。
    pub fn find_state(&self, state_id: &str) -> Option<&StateNode> {
        self.states.iter().find(|s| s.id == state_id)
    }

    /// 查找从指定状态出发的所有迁移。
    pub fn transitions_from(&self, state_id: &str) -> Vec<&Transition> {
        self.transitions
            .iter()
            .filter(|t| t.from == state_id)
            .collect()
    }
}

/// 状态类型。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StateType {
    /// 初始状态
    Initial,
    /// 普通状态
    Normal,
    /// 终止状态
    Terminal,
    /// 错误状态
    Error,
}

/// 状态节点。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StateNode {
    /// 状态 ID
    pub id: String,
    /// 状态类型
    #[serde(rename = "type")]
    pub state_type: StateType,
    /// 状态显示名称
    #[serde(default)]
    pub label: String,
    /// 进入该状态时执行的任务定义
    #[serde(default)]
    pub task: Option<TaskDef>,
}

/// 迁移定义。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Transition {
    /// 源状态 ID
    pub from: String,
    /// 目标状态 ID
    pub to: String,
    /// 迁移条件
    #[serde(default)]
    pub condition: TransitionCondition,
}

/// 迁移条件。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TransitionCondition {
    /// 无条件通过
    Always,
    /// 表达式条件（运行时求值，当前实现为简化比较）
    Expression { expr: String },
    /// 变量匹配条件
    VariableEquals { key: String, value: serde_json::Value },
}

impl Default for TransitionCondition {
    fn default() -> Self {
        TransitionCondition::Always
    }
}

impl TransitionCondition {
    /// 基于上下文评估条件是否满足。
    pub fn evaluate(&self, ctx: &WorkflowContext) -> bool {
        match self {
            TransitionCondition::Always => true,
            TransitionCondition::Expression { expr } => {
                // 简化实现：若表达式形如 "var.key == 'value'" 则做基础解析
                // 实际生产环境可引入专用表达式引擎
                evaluate_simple_expression(expr, ctx)
            }
            TransitionCondition::VariableEquals { key, value } => {
                ctx.variables.get(key) == Some(value)
            }
        }
    }
}

fn evaluate_simple_expression(expr: &str, ctx: &WorkflowContext) -> bool {
    // 极简表达式求值：仅支持 "result.status == 'ok'" 形式
    let parts: Vec<&str> = expr.split("==").collect();
    if parts.len() != 2 {
        return false;
    }
    let left = parts[0].trim();
    let right = parts[1].trim().trim_matches('\'').trim_matches('"');
    if let Some((obj, field)) = left.split_once('.') {
        if obj == "result" {
            if let Some(val) = ctx.result.get(field) {
                return val.as_str() == Some(right);
            }
        }
    }
    false
}

/// 任务定义。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskDef {
    /// 任务类型标识
    pub task_type: String,
    /// 任务参数
    #[serde(default)]
    pub params: HashMap<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// 工作流实例与上下文
// ---------------------------------------------------------------------------

/// 工作流实例状态。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InstanceStatus {
    /// 运行中
    Running,
    /// 已完成
    Completed,
    /// 已失败
    Failed,
    /// 已挂起
    Suspended,
}

/// 工作流实例。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowInstance {
    /// 实例 ID
    pub instance_id: String,
    /// 关联的工作流定义 ID
    pub workflow_id: String,
    /// 当前状态 ID
    pub current_state: String,
    /// 实例状态
    pub status: InstanceStatus,
    /// 运行时上下文
    pub context: WorkflowContext,
    /// 创建时间
    pub created_at: DateTime<Utc>,
    /// 更新时间
    pub updated_at: DateTime<Utc>,
    /// 历史轨迹
    #[serde(default)]
    pub history: Vec<StateHistoryEntry>,
}

impl WorkflowInstance {
    /// 基于定义创建新实例。
    pub fn new(workflow_id: &str, initial_state: &str) -> Self {
        let now = Utc::now();
        Self {
            instance_id: uuid::Uuid::new_v4().to_string(),
            workflow_id: workflow_id.to_string(),
            current_state: initial_state.to_string(),
            status: InstanceStatus::Running,
            context: WorkflowContext::default(),
            created_at: now,
            updated_at: now,
            history: vec![StateHistoryEntry {
                state_id: initial_state.to_string(),
                entered_at: now,
                exited_at: None,
            }],
        }
    }

    /// 推进到下一状态。
    pub fn transition_to(&mut self, next_state: &str) {
        let now = Utc::now();
        if let Some(last) = self.history.last_mut() {
            last.exited_at = Some(now);
        }
        self.current_state = next_state.to_string();
        self.updated_at = now;
        self.history.push(StateHistoryEntry {
            state_id: next_state.to_string(),
            entered_at: now,
            exited_at: None,
        });
    }

    /// 标记为完成。
    pub fn complete(&mut self) {
        self.status = InstanceStatus::Completed;
        self.updated_at = Utc::now();
    }

    /// 标记为失败。
    pub fn fail(&mut self) {
        self.status = InstanceStatus::Failed;
        self.updated_at = Utc::now();
    }
}

/// 状态历史条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateHistoryEntry {
    pub state_id: String,
    pub entered_at: DateTime<Utc>,
    pub exited_at: Option<DateTime<Utc>>,
}

/// 工作流运行时上下文。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkflowContext {
    /// 用户自定义变量
    #[serde(default)]
    pub variables: HashMap<String, serde_json::Value>,
    /// 最近一次任务执行结果
    #[serde(default)]
    pub result: HashMap<String, serde_json::Value>,
    /// 触发器携带的输入数据
    #[serde(default)]
    pub input: serde_json::Value,
}

// ---------------------------------------------------------------------------
// 触发器
// ---------------------------------------------------------------------------

/// 触发器类型。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TriggerType {
    /// 时间触发（支持 cron 表达式或绝对时间）
    Schedule {
        /// cron 表达式（可选，与 `at` 二选一）
        #[serde(default)]
        cron: Option<String>,
        /// 绝对触发时间（可选）
        #[serde(default)]
        at: Option<DateTime<Utc>>,
    },
    /// 事件触发（监听 CoreEvent 或内部事件）
    Event {
        /// 监听的事件类型过滤
        event_type: String,
        /// 可选过滤条件（JSON 对象）
        #[serde(default)]
        filter: Option<serde_json::Value>,
    },
    /// API 触发（外部 HTTP/RPC 调用）
    Api {
        /// API 端点标识
        endpoint: String,
    },
}

/// 触发器定义。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trigger {
    /// 触发器 ID
    pub trigger_id: String,
    /// 关联工作流定义 ID
    pub workflow_id: String,
    /// 触发器类型
    #[serde(flatten)]
    pub trigger_type: TriggerType,
    /// 是否启用
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// 创建时间
    pub created_at: DateTime<Utc>,
}

fn default_true() -> bool {
    true
}

impl Trigger {
    /// 创建一个新的时间触发器。
    pub fn schedule(workflow_id: &str, cron: Option<String>, at: Option<DateTime<Utc>>) -> Self {
        Self {
            trigger_id: uuid::Uuid::new_v4().to_string(),
            workflow_id: workflow_id.to_string(),
            trigger_type: TriggerType::Schedule { cron, at },
            enabled: true,
            created_at: Utc::now(),
        }
    }

    /// 创建一个新的事件触发器。
    pub fn event(workflow_id: &str, event_type: &str, filter: Option<serde_json::Value>) -> Self {
        Self {
            trigger_id: uuid::Uuid::new_v4().to_string(),
            workflow_id: workflow_id.to_string(),
            trigger_type: TriggerType::Event {
                event_type: event_type.to_string(),
                filter,
            },
            enabled: true,
            created_at: Utc::now(),
        }
    }

    /// 创建一个新的 API 触发器。
    pub fn api(workflow_id: &str, endpoint: &str) -> Self {
        Self {
            trigger_id: uuid::Uuid::new_v4().to_string(),
            workflow_id: workflow_id.to_string(),
            trigger_type: TriggerType::Api {
                endpoint: endpoint.to_string(),
            },
            enabled: true,
            created_at: Utc::now(),
        }
    }
}

// ---------------------------------------------------------------------------
// 异步任务执行器（含重试与死信队列）
// ---------------------------------------------------------------------------

/// 执行任务请求。
#[derive(Debug, Clone)]
pub struct TaskRequest {
    /// 任务 ID
    pub task_id: String,
    /// 关联工作流实例 ID
    pub instance_id: String,
    /// 任务定义
    pub task_def: TaskDef,
    /// 当前重试次数
    pub retry_count: u32,
    /// 最大重试次数
    pub max_retries: u32,
}

impl TaskRequest {
    /// 创建任务请求。
    pub fn new(instance_id: &str, task_def: TaskDef, max_retries: u32) -> Self {
        Self {
            task_id: uuid::Uuid::new_v4().to_string(),
            instance_id: instance_id.to_string(),
            task_def,
            retry_count: 0,
            max_retries,
        }
    }
}

/// 任务执行结果。
#[derive(Debug, Clone)]
pub enum TaskResult {
    /// 成功，携带输出数据
    Success(serde_json::Value),
    /// 失败，可重试
    Retryable(String),
    /// 失败，不可重试
    Fatal(String),
}

/// 死信条目。
#[derive(Debug, Clone)]
pub struct DeadLetterEntry {
    /// 原始任务请求
    pub task: TaskRequest,
    /// 最终失败原因
    pub reason: String,
    /// 进入死信队列时间
    pub dead_at: DateTime<Utc>,
}

/// 异步任务执行器。
///
/// 内部使用 `tokio::sync::mpsc` 队列驱动 worker 消费任务，
/// 支持指数退避重试，超过最大重试次数后进入死信队列。
pub struct TaskExecutor {
    /// 任务发送端
    sender: mpsc::UnboundedSender<TaskRequest>,
    /// 死信队列（线程安全）
    dead_letter_queue: Arc<Mutex<Vec<DeadLetterEntry>>>,
    /// 运行时结果缓存
    results: Arc<Mutex<HashMap<String, TaskResult>>>,
}

impl TaskExecutor {
    /// 创建任务执行器并启动后台 worker。
    pub fn new() -> Self {
        let (sender, mut receiver) = mpsc::unbounded_channel::<TaskRequest>();
        let dlq = Arc::new(Mutex::new(Vec::new()));
        let results = Arc::new(Mutex::new(HashMap::new()));

        let dlq_worker = Arc::clone(&dlq);
        let results_worker = Arc::clone(&results);

        tokio::spawn(async move {
            while let Some(mut task) = receiver.recv().await {
                debug!(task_id = %task.task_id, instance_id = %task.instance_id, "executing task");

                match execute_task(&task).await {
                    TaskResult::Success(val) => {
                        info!(task_id = %task.task_id, "task succeeded");
                        results_worker
                            .lock()
                            .insert(task.task_id.clone(), TaskResult::Success(val));
                    }
                    TaskResult::Retryable(err) => {
                        if task.retry_count < task.max_retries {
                            task.retry_count += 1;
                            let backoff = std::time::Duration::from_millis(
                                100 * 2_u64.pow(task.retry_count),
                            );
                            warn!(
                                task_id = %task.task_id,
                                retry = task.retry_count,
                                ?backoff,
                                error = %err,
                                "task failed, will retry"
                            );
                            tokio::time::sleep(backoff).await;
                            // 重新投递到同一通道（当前为 unbounded，生产环境建议 bounded）
                            // 注：因 receiver 在此 worker 中，无法直接 send 回同一 channel。
                            // 为保持简单，此处将重试任务就地重新执行。
                            // 若需跨 worker 重试，可将 sender 通过 Arc 共享。
                            // 下面使用就地循环模拟重试逻辑：
                            let mut current = task;
                            loop {
                                match execute_task(&current).await {
                                    TaskResult::Success(val) => {
                                        info!(task_id = %current.task_id, "retry succeeded");
                                        results_worker.lock().insert(
                                            current.task_id.clone(),
                                            TaskResult::Success(val),
                                        );
                                        break;
                                    }
                                    TaskResult::Retryable(e) => {
                                        if current.retry_count < current.max_retries {
                                            current.retry_count += 1;
                                            let bo = std::time::Duration::from_millis(
                                                100 * 2_u64.pow(current.retry_count),
                                            );
                                            warn!(
                                                task_id = %current.task_id,
                                                retry = current.retry_count,
                                                ?bo,
                                                error = %e,
                                                "retry failed again"
                                            );
                                            tokio::time::sleep(bo).await;
                                        } else {
                                            error!(task_id = %current.task_id, error = %e, "task exhausted retries");
                                            dlq_worker.lock().push(DeadLetterEntry {
                                                task: current,
                                                reason: e,
                                                dead_at: Utc::now(),
                                            });
                                            break;
                                        }
                                    }
                                    TaskResult::Fatal(e) => {
                                        error!(task_id = %current.task_id, error = %e, "task fatal error");
                                        dlq_worker.lock().push(DeadLetterEntry {
                                            task: current,
                                            reason: e,
                                            dead_at: Utc::now(),
                                        });
                                        break;
                                    }
                                }
                            }
                        } else {
                            error!(task_id = %task.task_id, error = %err, "task exhausted retries");
                            dlq_worker.lock().push(DeadLetterEntry {
                                task,
                                reason: err,
                                dead_at: Utc::now(),
                            });
                        }
                    }
                    TaskResult::Fatal(err) => {
                        error!(task_id = %task.task_id, error = %err, "task fatal error");
                        dlq_worker.lock().push(DeadLetterEntry {
                            task,
                            reason: err,
                            dead_at: Utc::now(),
                        });
                    }
                }
            }
        });

        Self {
            sender,
            dead_letter_queue: dlq,
            results,
        }
    }

    /// 提交任务到执行队列。
    pub fn submit(&self, task: TaskRequest) {
        let _ = self.sender.send(task);
    }

    /// 获取死信队列当前长度。
    pub fn dead_letter_count(&self) -> usize {
        self.dead_letter_queue.lock().len()
    }

    /// 读取并清空死信队列。
    pub fn drain_dead_letters(&self) -> Vec<DeadLetterEntry> {
        std::mem::take(&mut *self.dead_letter_queue.lock())
    }

    /// 获取任务结果（若已执行完毕）。
    pub fn get_result(&self, task_id: &str) -> Option<TaskResult> {
        self.results.lock().get(task_id).cloned()
    }
}

impl Default for TaskExecutor {
    fn default() -> Self {
        Self::new()
    }
}

/// 模拟任务执行。生产环境可替换为真实 HTTP 调用、WASM 插件调用等。
async fn execute_task(task: &TaskRequest) -> TaskResult {
    // 模拟异步 I/O
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    match task.task_def.task_type.as_str() {
        "noop" => TaskResult::Success(serde_json::json!({"status": "ok"})),
        "fail_once" => {
            if task.retry_count == 0 {
                TaskResult::Retryable("simulated transient failure".to_string())
            } else {
                TaskResult::Success(serde_json::json!({"status": "recovered"}))
            }
        }
        "always_fail" => TaskResult::Retryable("persistent failure".to_string()),
        "fatal" => TaskResult::Fatal("unrecoverable error".to_string()),
        _ => TaskResult::Success(serde_json::json!({"status": "ok"})),
    }
}

// ---------------------------------------------------------------------------
// 工作流引擎
// ---------------------------------------------------------------------------

/// 工作流引擎，统筹定义、实例、触发器与任务执行。
pub struct WorkflowEngine {
    /// 工作流定义仓库
    definitions: Arc<Mutex<HashMap<String, WorkflowDefinition>>>,
    /// 工作流实例仓库
    instances: Arc<Mutex<HashMap<String, WorkflowInstance>>>,
    /// 触发器仓库
    triggers: Arc<Mutex<HashMap<String, Trigger>>>,
    /// 任务执行器
    executor: TaskExecutor,
}

impl WorkflowEngine {
    /// 创建新的工作流引擎。
    pub fn new() -> Self {
        Self {
            definitions: Arc::new(Mutex::new(HashMap::new())),
            instances: Arc::new(Mutex::new(HashMap::new())),
            triggers: Arc::new(Mutex::new(HashMap::new())),
            executor: TaskExecutor::new(),
        }
    }

    /// 注册工作流定义。
    pub fn register_definition(&self, def: WorkflowDefinition) {
        info!(workflow_id = %def.id, "registering workflow definition");
        self.definitions.lock().insert(def.id.clone(), def);
    }

    /// 获取定义。
    pub fn get_definition(&self, id: &str) -> Option<WorkflowDefinition> {
        self.definitions.lock().get(id).cloned()
    }

    /// 启动一个新的工作流实例。
    pub fn start_instance(&self, workflow_id: &str, input: serde_json::Value) -> Result<WorkflowInstance, Error> {
        let defs = self.definitions.lock();
        let def = defs
            .get(workflow_id)
            .ok_or_else(|| Error::NotFound(format!("workflow definition not found: {}", workflow_id)))?;
        let initial = def
            .initial_state()
            .ok_or_else(|| Error::InvalidInput("workflow has no initial state".to_string()))?;

        let mut instance = WorkflowInstance::new(workflow_id, &initial.id);
        instance.context.input = input;

        info!(instance_id = %instance.instance_id, workflow_id = %workflow_id, "starting workflow instance");
        self.instances.lock().insert(instance.instance_id.clone(), instance.clone());
        Ok(instance)
    }

    /// 获取实例。
    pub fn get_instance(&self, instance_id: &str) -> Option<WorkflowInstance> {
        self.instances.lock().get(instance_id).cloned()
    }

    /// 推进实例到下一状态（由外部调用或定时轮询触发）。
    ///
    /// 若当前状态有关联任务，先将任务提交到执行器；否则立即评估迁移条件。
    pub fn step_instance(&self, instance_id: &str) -> Result<Option<String>, Error> {
        let mut instances = self.instances.lock();
        let instance = instances
            .get_mut(instance_id)
            .ok_or_else(|| Error::NotFound(format!("instance not found: {}", instance_id)))?;

        if instance.status != InstanceStatus::Running {
            return Ok(None);
        }

        let defs = self.definitions.lock();
        let def = defs
            .get(&instance.workflow_id)
            .ok_or_else(|| Error::NotFound("workflow definition missing".to_string()))?;

        let current_state = def
            .find_state(&instance.current_state)
            .ok_or_else(|| Error::Internal("current state missing in definition".to_string()))?;

        // 若状态有任务定义，提交异步执行（简化：直接提交，不等待）
        if let Some(task_def) = &current_state.task {
            let task = TaskRequest::new(&instance.instance_id, task_def.clone(), 3);
            self.executor.submit(task);
            // 简化：不阻塞等待任务完成，实际生产可监听结果通道再推进
        }

        // 评估迁移条件
        let candidates = def.transitions_from(&instance.current_state);
        for transition in candidates {
            if transition.condition.evaluate(&instance.context) {
                let next = transition.to.clone();
                instance.transition_to(&next);

                let next_state = def.find_state(&next).ok_or_else(|| Error::Internal("target state missing".to_string()))?;
                if next_state.state_type == StateType::Terminal {
                    instance.complete();
                } else if next_state.state_type == StateType::Error {
                    instance.fail();
                }

                return Ok(Some(next));
            }
        }

        Ok(None)
    }

    /// 注册触发器。
    pub fn register_trigger(&self, trigger: Trigger) {
        info!(trigger_id = %trigger.trigger_id, workflow_id = %trigger.workflow_id, "registering trigger");
        self.triggers.lock().insert(trigger.trigger_id.clone(), trigger);
    }

    /// 获取触发器。
    pub fn get_trigger(&self, trigger_id: &str) -> Option<Trigger> {
        self.triggers.lock().get(trigger_id).cloned()
    }

    /// 按工作流 ID 列出触发器。
    pub fn list_triggers(&self, workflow_id: &str) -> Vec<Trigger> {
        self.triggers
            .lock()
            .values()
            .filter(|t| t.workflow_id == workflow_id)
            .cloned()
            .collect()
    }

    /// 获取任务执行器引用。
    pub fn executor(&self) -> &TaskExecutor {
        &self.executor
    }

    /// 获取死信队列条目数。
    pub fn dead_letter_count(&self) -> usize {
        self.executor.dead_letter_count()
    }
}

impl Default for WorkflowEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_workflow_json() -> &'static str {
        r#"
        {
            "id": "wf-approval",
            "name": "Document Approval",
            "states": [
                {"id": "start", "type": "initial", "label": "Start"},
                {"id": "review", "type": "normal", "label": "Under Review", "task": {"task_type": "noop"}},
                {"id": "approved", "type": "terminal", "label": "Approved"},
                {"id": "rejected", "type": "terminal", "label": "Rejected"}
            ],
            "transitions": [
                {"from": "start", "to": "review", "condition": {"type": "always"}},
                {"from": "review", "to": "approved", "condition": {"type": "variable_equals", "key": "decision", "value": "approve"}},
                {"from": "review", "to": "rejected", "condition": {"type": "variable_equals", "key": "decision", "value": "reject"}}
            ]
        }
        "#
    }

    #[test]
    fn test_parse_workflow_definition() {
        let def = WorkflowDefinition::from_json(sample_workflow_json()).unwrap();
        assert_eq!(def.id, "wf-approval");
        assert_eq!(def.states.len(), 4);
        assert_eq!(def.transitions.len(), 3);
        assert_eq!(def.initial_state().unwrap().id, "start");
    }

    #[tokio::test]
    async fn test_workflow_instance_lifecycle() {
        let def = WorkflowDefinition::from_json(sample_workflow_json()).unwrap();
        let engine = WorkflowEngine::new();
        engine.register_definition(def);

        let instance = engine
            .start_instance("wf-approval", serde_json::json!({"doc_id": "doc-1"}))
            .unwrap();
        assert_eq!(instance.current_state, "start");
        assert_eq!(instance.status, InstanceStatus::Running);

        // step 1: start -> review
        let next = engine.step_instance(&instance.instance_id).unwrap();
        assert_eq!(next, Some("review".to_string()));
        let inst = engine.get_instance(&instance.instance_id).unwrap();
        assert_eq!(inst.current_state, "review");

        // 无匹配条件，应停留在 review
        let next = engine.step_instance(&instance.instance_id).unwrap();
        assert_eq!(next, None);

        // 设置变量后再次推进
        {
            let mut instances = engine.instances.lock();
            let inst = instances.get_mut(&instance.instance_id).unwrap();
            inst.context
                .variables
                .insert("decision".to_string(), serde_json::json!("approve"));
        }

        let next = engine.step_instance(&instance.instance_id).unwrap();
        assert_eq!(next, Some("approved".to_string()));
        let inst = engine.get_instance(&instance.instance_id).unwrap();
        assert_eq!(inst.status, InstanceStatus::Completed);
    }

    #[test]
    fn test_trigger_creation() {
        let t1 = Trigger::schedule("wf-1", Some("0 9 * * *".to_string()), None);
        assert!(matches!(t1.trigger_type, TriggerType::Schedule { .. }));
        assert!(t1.enabled);

        let t2 = Trigger::event("wf-1", "DocumentChanged", None);
        assert!(matches!(t2.trigger_type, TriggerType::Event { .. }));

        let t3 = Trigger::api("wf-1", "/api/v1/run");
        assert!(matches!(t3.trigger_type, TriggerType::Api { .. }));
    }

    #[tokio::test]
    async fn test_task_executor_success() {
        let executor = TaskExecutor::new();
        let task = TaskRequest::new(
            "inst-1",
            TaskDef {
                task_type: "noop".to_string(),
                params: HashMap::new(),
            },
            3,
        );
        let tid = task.task_id.clone();
        executor.submit(task);

        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        let result = executor.get_result(&tid);
        assert!(matches!(result, Some(TaskResult::Success(_))));
        assert_eq!(executor.dead_letter_count(), 0);
    }

    #[tokio::test]
    async fn test_task_executor_retry_and_dlq() {
        let executor = TaskExecutor::new();
        let task = TaskRequest::new(
            "inst-1",
            TaskDef {
                task_type: "always_fail".to_string(),
                params: HashMap::new(),
            },
            2,
        );
        let tid = task.task_id.clone();
        executor.submit(task);

        // 等待重试耗尽
        tokio::time::sleep(tokio::time::Duration::from_millis(800)).await;
        let result = executor.get_result(&tid);
        assert!(result.is_none() || matches!(result, Some(TaskResult::Retryable(_))));
        assert_eq!(executor.dead_letter_count(), 1);

        let dlq = executor.drain_dead_letters();
        assert_eq!(dlq.len(), 1);
        assert_eq!(dlq[0].task.task_id, tid);
    }

    #[tokio::test]
    async fn test_task_executor_fatal_to_dlq() {
        let executor = TaskExecutor::new();
        let task = TaskRequest::new(
            "inst-1",
            TaskDef {
                task_type: "fatal".to_string(),
                params: HashMap::new(),
            },
            3,
        );
        executor.submit(task);

        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        assert_eq!(executor.dead_letter_count(), 1);
    }

    #[test]
    fn test_transition_condition_evaluate() {
        let mut ctx = WorkflowContext::default();
        ctx.variables
            .insert("decision".to_string(), serde_json::json!("approve"));

        let cond = TransitionCondition::VariableEquals {
            key: "decision".to_string(),
            value: serde_json::json!("approve"),
        };
        assert!(cond.evaluate(&ctx));

        let cond2 = TransitionCondition::VariableEquals {
            key: "decision".to_string(),
            value: serde_json::json!("reject"),
        };
        assert!(!cond2.evaluate(&ctx));

        let cond3 = TransitionCondition::Always;
        assert!(cond3.evaluate(&ctx));
    }

    #[test]
    fn test_expression_evaluation() {
        let mut ctx = WorkflowContext::default();
        ctx.result
            .insert("status".to_string(), serde_json::json!("ok"));

        let cond = TransitionCondition::Expression {
            expr: "result.status == 'ok'".to_string(),
        };
        assert!(cond.evaluate(&ctx));

        let cond2 = TransitionCondition::Expression {
            expr: "result.status == 'fail'".to_string(),
        };
        assert!(!cond2.evaluate(&ctx));
    }
}
