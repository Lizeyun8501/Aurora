//! Agent 编排 (Agent Orchestration)
//!
//! 支持三种编排模式：
//! - **Sequential**：链式执行，前一步输出作为后一步输入。
//! - **Parallel**：扇出并发执行，再汇聚收集全部结果。
//! - **Hierarchical**：树形编排，根节点产出后递归驱动子节点。
//!
//! 同时提供 LLM 生成的执行计划（以确定性规划器 mock），以及可序列化的
//! `PlanGraph` 用于可视化展示。
//!
//! # 关键类型
//! - [`OrchestrationMode`]：三种模式枚举。
//! - [`PlanStep`] / [`PlanNode`]：计划步骤与树节点。
//! - [`OrchestrationPlan`] / [`PlanGraph`]：完整计划与可视化图。
//! - [`AgentOrchestrator`]：编排器，持有 `ToolRegistry` 并执行计划。
//! - [`DeterministicPlanner`]：基于关键字的确定性 LLM 替身。

use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::registry::{ToolInvocation, ToolRegistry, ToolResult};

// ============================================================================
// 编排模式与计划结构
// ============================================================================

/// 编排模式。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum OrchestrationMode {
    /// 顺序链：step1 -> step2 -> step3
    Sequential,
    /// 并行扇出：step1, step2, step3 同时执行后汇聚
    Parallel,
    /// 层级树：root -> children[]
    Hierarchical,
}

impl OrchestrationMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            OrchestrationMode::Sequential => "sequential",
            OrchestrationMode::Parallel => "parallel",
            OrchestrationMode::Hierarchical => "hierarchical",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "sequential" => Some(OrchestrationMode::Sequential),
            "parallel" => Some(OrchestrationMode::Parallel),
            "hierarchical" => Some(OrchestrationMode::Hierarchical),
            _ => None,
        }
    }
}

/// 计划步骤。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    /// 步骤唯一 ID
    pub id: String,
    /// 调用的工具名
    pub tool_name: String,
    /// 调用参数（可含上游结果占位符）
    pub arguments: serde_json::Value,
    /// 依赖的上游步骤 ID 列表
    pub depends_on: Vec<String>,
}

impl PlanStep {
    pub fn new(
        id: impl Into<String>,
        tool_name: impl Into<String>,
        arguments: serde_json::Value,
    ) -> Self {
        Self {
            id: id.into(),
            tool_name: tool_name.into(),
            arguments,
            depends_on: Vec::new(),
        }
    }

    pub fn with_depends_on(mut self, deps: Vec<String>) -> Self {
        self.depends_on = deps;
        self
    }
}

/// 树形节点（用于层级模式）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanNode {
    pub step: PlanStep,
    pub children: Vec<PlanNode>,
}

impl PlanNode {
    pub fn new(step: PlanStep) -> Self {
        Self {
            step,
            children: Vec::new(),
        }
    }

    pub fn with_children(mut self, children: Vec<PlanNode>) -> Self {
        self.children = children;
        self
    }

    /// 子节点数。
    pub fn child_count(&self) -> usize {
        self.children.len()
    }

    /// 递归统计节点总数。
    pub fn total_nodes(&self) -> usize {
        1 + self.children.iter().map(|c| c.total_nodes()).sum::<usize>()
    }

    /// 递归深度。
    pub fn depth(&self) -> usize {
        1 + self
            .children
            .iter()
            .map(|c| c.depth())
            .max()
            .unwrap_or(0)
    }
}

/// 计划图边（用于可视化）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanEdge {
    pub from: String,
    pub to: String,
    pub label: Option<String>,
}

impl PlanEdge {
    pub fn new(from: impl Into<String>, to: impl Into<String>) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
            label: None,
        }
    }
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

/// 计划图：节点 + 边的可序列化结构，便于前端可视化。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanGraph {
    pub mode: OrchestrationMode,
    pub nodes: Vec<PlanStep>,
    pub edges: Vec<PlanEdge>,
    pub root_id: Option<String>,
}

impl PlanGraph {
    pub fn new(mode: OrchestrationMode) -> Self {
        Self {
            mode,
            nodes: Vec::new(),
            edges: Vec::new(),
            root_id: None,
        }
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// 序列化为 JSON 字符串（供可视化前端使用）。
    pub fn to_json(&self) -> Result<String, crate::Error> {
        serde_json::to_string_pretty(self).map_err(crate::Error::from)
    }
}

/// 编排计划：完整执行蓝图。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationPlan {
    pub mode: OrchestrationMode,
    pub steps: Vec<PlanStep>,
    /// 层级模式的根节点（其他模式为 None）
    pub root: Option<PlanNode>,
    pub session_id: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl OrchestrationPlan {
    pub fn new(mode: OrchestrationMode, session_id: impl Into<String>) -> Self {
        Self {
            mode,
            steps: Vec::new(),
            root: None,
            session_id: session_id.into(),
            created_at: chrono::Utc::now(),
        }
    }

    pub fn with_steps(mut self, steps: Vec<PlanStep>) -> Self {
        self.steps = steps;
        self
    }

    pub fn with_root(mut self, root: PlanNode) -> Self {
        self.root = Some(root);
        self
    }

    /// 步骤数。
    pub fn step_count(&self) -> usize {
        self.steps.len()
    }

    /// 生成对应的可视化图。
    pub fn to_graph(&self) -> PlanGraph {
        let mut graph = PlanGraph::new(self.mode);
        match self.mode {
            OrchestrationMode::Sequential | OrchestrationMode::Parallel => {
                for step in &self.steps {
                    if graph.root_id.is_none() {
                        graph.root_id = Some(step.id.clone());
                    }
                    for dep in &step.depends_on {
                        graph.edges.push(PlanEdge::new(dep.clone(), step.id.clone()));
                    }
                    graph.nodes.push(step.clone());
                }
            }
            OrchestrationMode::Hierarchical => {
                if let Some(root) = &self.root {
                    graph.root_id = Some(root.step.id.clone());
                    flatten_node(root, &mut graph);
                }
            }
        }
        graph
    }
}

fn flatten_node(node: &PlanNode, graph: &mut PlanGraph) {
    graph.nodes.push(node.step.clone());
    for child in &node.children {
        graph.edges.push(PlanEdge::new(node.step.id.clone(), child.step.id.clone()));
        flatten_node(child, graph);
    }
}

// ============================================================================
// 编排结果
// ============================================================================

/// 单步执行结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    pub step_id: String,
    pub tool_name: String,
    pub result: ToolResult,
}

/// 编排执行总结。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationSummary {
    pub mode: OrchestrationMode,
    pub session_id: String,
    pub step_results: Vec<StepResult>,
    /// 顺序模式下的最终输出
    pub final_output: Option<serde_json::Value>,
    pub total_latency_ms: u64,
    pub success: bool,
}

// ============================================================================
// 编排器
// ============================================================================

/// Agent 编排器：持有 `ToolRegistry` 并按计划执行。
pub struct AgentOrchestrator {
    registry: Arc<ToolRegistry>,
    history: Arc<RwLock<Vec<OrchestrationSummary>>>,
}

impl AgentOrchestrator {
    pub fn new(registry: Arc<ToolRegistry>) -> Self {
        Self {
            registry,
            history: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// 执行一个计划，返回总结。
    pub async fn execute(&self, plan: &OrchestrationPlan) -> Result<OrchestrationSummary, crate::Error> {
        info!(
            "orchestrator: execute plan mode={:?} steps={}",
            plan.mode,
            plan.step_count()
        );
        let started = std::time::Instant::now();
        let (step_results, final_output) = match plan.mode {
            OrchestrationMode::Sequential => self.execute_sequential(plan).await?,
            OrchestrationMode::Parallel => self.execute_parallel(plan).await?,
            OrchestrationMode::Hierarchical => self.execute_hierarchical(plan).await?,
        };
        let total_latency = started.elapsed().as_millis() as u64;
        let success = step_results.iter().all(|r| r.result.is_ok());
        let summary = OrchestrationSummary {
            mode: plan.mode,
            session_id: plan.session_id.clone(),
            step_results,
            final_output,
            total_latency_ms: total_latency,
            success,
        };
        self.history.write().push(summary.clone());
        Ok(summary)
    }

    /// 顺序执行：链式传递前一步结果。
    async fn execute_sequential(
        &self,
        plan: &OrchestrationPlan,
    ) -> Result<(Vec<StepResult>, Option<serde_json::Value>), crate::Error> {
        let mut results = Vec::new();
        let mut prev_output: Option<serde_json::Value> = None;
        for step in &plan.steps {
            // 将上一步输出注入到当前参数（若占位 "$prev" 存在）
            let arguments = inject_prev(&step.arguments, prev_output.as_ref());
            let inv = ToolInvocation::new(
                step.tool_name.clone(),
                arguments,
                &plan.session_id,
            );
            debug!("orchestrator[seq]: step {} tool {}", step.id, step.tool_name);
            let tr = self.registry.invoke(&inv).await?;
            results.push(StepResult {
                step_id: step.id.clone(),
                tool_name: step.tool_name.clone(),
                result: tr.clone(),
            });
            if tr.is_err() {
                // 链式失败：立即终止
                warn!("orchestrator[seq]: step {} failed, aborting chain", step.id);
                return Ok((results, None));
            }
            prev_output = Some(tr.output);
        }
        Ok((results, prev_output))
    }

    /// 并行执行：所有步骤独立调用，汇聚结果。
    async fn execute_parallel(
        &self,
        plan: &OrchestrationPlan,
    ) -> Result<(Vec<StepResult>, Option<serde_json::Value>), crate::Error> {
        let mut results = Vec::new();
        // 同步并发模拟：依次调用但语义上彼此独立（不依赖前一步输出）
        for step in &plan.steps {
            let inv = ToolInvocation::new(
                step.tool_name.clone(),
                step.arguments.clone(),
                &plan.session_id,
            );
            debug!("orchestrator[par]: step {} tool {}", step.id, step.tool_name);
            let tr = self.registry.invoke(&inv).await?;
            results.push(StepResult {
                step_id: step.id.clone(),
                tool_name: step.tool_name.clone(),
                result: tr,
            });
        }
        // 汇聚为对象 { step_id: output }
        let mut gathered = serde_json::Map::new();
        for r in &results {
            gathered.insert(r.step_id.clone(), r.result.output.clone());
        }
        Ok((results, Some(serde_json::Value::Object(gathered))))
    }

    /// 层级执行：递归遍历树。
    async fn execute_hierarchical(
        &self,
        plan: &OrchestrationPlan,
    ) -> Result<(Vec<StepResult>, Option<serde_json::Value>), crate::Error> {
        let mut results = Vec::new();
        if let Some(root) = &plan.root {
            let mut final_output = self.execute_node(root, plan, &mut results, None).await?;
            // 最终输出取根节点结果
            if final_output.is_none() && !results.is_empty() {
                final_output = Some(results[0].result.output.clone());
            }
            Ok((results, final_output))
        } else {
            Err(crate::Error::InvalidInput(
                "hierarchical plan requires root node".into(),
            ))
        }
    }

    async fn execute_node(
        &self,
        node: &PlanNode,
        plan: &OrchestrationPlan,
        results: &mut Vec<StepResult>,
        parent_output: Option<&serde_json::Value>,
    ) -> Result<Option<serde_json::Value>, crate::Error> {
        let arguments = inject_prev(&node.step.arguments, parent_output);
        let inv = ToolInvocation::new(
            node.step.tool_name.clone(),
            arguments,
            &plan.session_id,
        );
        debug!(
            "orchestrator[hier]: node {} tool {}",
            node.step.id, node.step.tool_name
        );
        let tr = self.registry.invoke(&inv).await?;
        let output = tr.output.clone();
        let is_err = tr.is_err();
        results.push(StepResult {
            step_id: node.step.id.clone(),
            tool_name: node.step.tool_name.clone(),
            result: tr,
        });
        if is_err {
            warn!(
                "orchestrator[hier]: node {} failed, skipping children",
                node.step.id
            );
            return Ok(None);
        }
        // 递归执行子节点
        for child in &node.children {
            Box::pin(self.execute_node(child, plan, results, Some(&output))).await?;
        }
        Ok(Some(output))
    }

    /// 历史执行快照。
    pub fn history(&self) -> Vec<OrchestrationSummary> {
        self.history.read().clone()
    }

    /// 底层注册表引用。
    pub fn registry(&self) -> &Arc<ToolRegistry> {
        &self.registry
    }
}

/// 把 `{"$prev": true}` 占位替换为上一步输出，递归处理对象/数组。
fn inject_prev(
    args: &serde_json::Value,
    prev: Option<&serde_json::Value>,
) -> serde_json::Value {
    match args {
        serde_json::Value::Object(map) => {
            // 整体替换：若对象只包含 "$prev": true
            if map.len() == 1 {
                if let Some(v) = map.get("$prev") {
                    if v.as_bool() == Some(true) {
                        if let Some(p) = prev {
                            return p.clone();
                        }
                        return serde_json::Value::Null;
                    }
                }
            }
            let mut new_map = serde_json::Map::new();
            for (k, v) in map {
                new_map.insert(k.clone(), inject_prev(v, prev));
            }
            serde_json::Value::Object(new_map)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(|v| inject_prev(v, prev)).collect())
        }
        other => other.clone(),
    }
}

// ============================================================================
// 确定性规划器（LLM mock）
// ============================================================================

/// 确定性规划器：基于任务描述中的关键字生成可执行计划。
///
/// 用作 LLM 的可重现替身，便于测试与离线编排：
/// - 含 `translate` → 顺序：extract → translate → format
/// - 含 `summarize` / `digest` → 顺序：fetch → summarize
/// - 含 `search` → 并行：search_web + search_local
/// - 含 `classify` → 层级：classify -> [enrich, tag]
/// - 其他 → 单步：default
pub struct DeterministicPlanner;

impl DeterministicPlanner {
    /// 根据任务描述生成计划。
    pub fn plan(&self, task: &str, session_id: &str) -> OrchestrationPlan {
        let lower = task.to_lowercase();
        if lower.contains("translate") {
            self.translate_plan(session_id)
        } else if lower.contains("summarize") || lower.contains("digest") {
            self.summarize_plan(session_id)
        } else if lower.contains("search") && !lower.contains("classify") {
            self.search_plan(session_id)
        } else if lower.contains("classify") {
            self.classify_plan(session_id)
        } else {
            self.default_plan(session_id)
        }
    }

    fn translate_plan(&self, session_id: &str) -> OrchestrationPlan {
        let steps = vec![
            PlanStep::new("s1", "extract", serde_json::json!({"source": "input"})),
            PlanStep::new("s2", "translate", serde_json::json!({"$prev": true}))
                .with_depends_on(vec!["s1".into()]),
            PlanStep::new("s3", "format", serde_json::json!({"$prev": true}))
                .with_depends_on(vec!["s2".into()]),
        ];
        OrchestrationPlan::new(OrchestrationMode::Sequential, session_id).with_steps(steps)
    }

    fn summarize_plan(&self, session_id: &str) -> OrchestrationPlan {
        let steps = vec![
            PlanStep::new("s1", "fetch", serde_json::json!({"url": "default"})),
            PlanStep::new("s2", "summarize", serde_json::json!({"$prev": true}))
                .with_depends_on(vec!["s1".into()]),
        ];
        OrchestrationPlan::new(OrchestrationMode::Sequential, session_id).with_steps(steps)
    }

    fn search_plan(&self, session_id: &str) -> OrchestrationPlan {
        let steps = vec![
            PlanStep::new("s1", "search_web", serde_json::json!({"q": "query"})),
            PlanStep::new("s2", "search_local", serde_json::json!({"q": "query"})),
        ];
        OrchestrationPlan::new(OrchestrationMode::Parallel, session_id).with_steps(steps)
    }

    fn classify_plan(&self, session_id: &str) -> OrchestrationPlan {
        let root = PlanNode::new(PlanStep::new("root", "classify", serde_json::json!({})))
            .with_children(vec![
                PlanNode::new(PlanStep::new("c1", "enrich", serde_json::json!({"$prev": true}))),
                PlanNode::new(PlanStep::new("c2", "tag", serde_json::json!({"$prev": true}))),
            ]);
        OrchestrationPlan::new(OrchestrationMode::Hierarchical, session_id)
            .with_root(root)
    }

    fn default_plan(&self, session_id: &str) -> OrchestrationPlan {
        let steps = vec![PlanStep::new("s1", "default", serde_json::json!({"task": "auto"}))];
        OrchestrationPlan::new(OrchestrationMode::Sequential, session_id).with_steps(steps)
    }
}

impl Default for DeterministicPlanner {
    fn default() -> Self {
        Self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::MockAgent;
    use aurora_core::traits::agent_protocol::AgentProtocol;

    fn make_orchestrator_with_tools() -> (AgentOrchestrator, Arc<ToolRegistry>) {
        let registry = Arc::new(ToolRegistry::new());
        let agent = Arc::new(MockAgent::new("a1"));
        // 注册一组工具处理器
        agent.register_handler("extract", |args| Ok(serde_json::json!({"extracted": args})));
        agent.register_handler("translate", |args| {
            Ok(serde_json::json!({"translated": args}))
        });
        agent.register_handler("format", |args| Ok(serde_json::json!({"formatted": args})));
        agent.register_handler("fetch", |_args| Ok(serde_json::json!({"fetched": "data"})));
        agent.register_handler("summarize", |args| Ok(serde_json::json!({"summary": args})));
        agent.register_handler("search_web", |_args| Ok(serde_json::json!({"web": 1})));
        agent.register_handler("search_local", |_args| Ok(serde_json::json!({"local": 2})));
        agent.register_handler("classify", |_args| Ok(serde_json::json!({"label": "A"})));
        agent.register_handler("enrich", |args| Ok(serde_json::json!({"enriched": args})));
        agent.register_handler("tag", |args| Ok(serde_json::json!({"tagged": args})));
        agent.register_handler("default", |args| Ok(serde_json::json!({"default": args})));
        agent.register_handler("fail", |_| Err("boom".to_string()));
        registry.register_agent("a1", agent as Arc<dyn AgentProtocol>);
        // 注册工具到 registry
        for name in [
            "extract",
            "translate",
            "format",
            "fetch",
            "summarize",
            "search_web",
            "search_local",
            "classify",
            "enrich",
            "tag",
            "default",
            "fail",
        ] {
            registry
                .register_tool(
                    crate::registry::Tool::new(name, format!("{} tool", name), serde_json::json!({}))
                        .with_agent("a1"),
                )
                .unwrap();
        }
        let orch = AgentOrchestrator::new(registry.clone());
        (orch, registry)
    }

    #[test]
    fn test_orchestration_mode_as_str_roundtrip() {
        for m in [
            OrchestrationMode::Sequential,
            OrchestrationMode::Parallel,
            OrchestrationMode::Hierarchical,
        ] {
            assert_eq!(OrchestrationMode::from_str(m.as_str()), Some(m));
        }
        assert_eq!(OrchestrationMode::from_str("unknown"), None);
    }

    #[test]
    fn test_plan_step_with_depends_on() {
        let step = PlanStep::new("s1", "echo", serde_json::json!({}))
            .with_depends_on(vec!["s0".into()]);
        assert_eq!(step.id, "s1");
        assert_eq!(step.depends_on, vec!["s0".to_string()]);
    }

    #[test]
    fn test_plan_node_total_nodes_and_depth() {
        let root = PlanNode::new(PlanStep::new("r", "t", serde_json::json!({}))).with_children(vec![
            PlanNode::new(PlanStep::new("c1", "t", serde_json::json!({}))),
            PlanNode::new(PlanStep::new("c2", "t", serde_json::json!({})))
                .with_children(vec![PlanNode::new(PlanStep::new(
                    "g1",
                    "t",
                    serde_json::json!({}),
                ))]),
        ]);
        assert_eq!(root.child_count(), 2);
        assert_eq!(root.total_nodes(), 4);
        assert_eq!(root.depth(), 3);
    }

    #[test]
    fn test_plan_edge_with_label() {
        let e = PlanEdge::new("a", "b").with_label("data");
        assert_eq!(e.from, "a");
        assert_eq!(e.to, "b");
        assert_eq!(e.label.as_deref(), Some("data"));
    }

    #[test]
    fn test_plan_graph_to_json() {
        let mut graph = PlanGraph::new(OrchestrationMode::Sequential);
        graph.nodes.push(PlanStep::new("s1", "echo", serde_json::json!({})));
        let json = graph.to_json().unwrap();
        assert!(json.contains("\"mode\""));
        assert!(json.contains("\"s1\""));
    }

    #[test]
    fn test_orchestration_plan_to_graph_sequential() {
        let plan = OrchestrationPlan::new(OrchestrationMode::Sequential, "s1")
            .with_steps(vec![
                PlanStep::new("s1", "a", serde_json::json!({})),
                PlanStep::new("s2", "b", serde_json::json!({})).with_depends_on(vec!["s1".into()]),
                PlanStep::new("s3", "c", serde_json::json!({})).with_depends_on(vec!["s2".into()]),
            ]);
        let graph = plan.to_graph();
        assert_eq!(graph.node_count(), 3);
        assert_eq!(graph.edge_count(), 2);
        assert_eq!(graph.root_id.as_deref(), Some("s1"));
    }

    #[test]
    fn test_orchestration_plan_to_graph_parallel() {
        let plan = OrchestrationPlan::new(OrchestrationMode::Parallel, "s1")
            .with_steps(vec![
                PlanStep::new("s1", "a", serde_json::json!({})),
                PlanStep::new("s2", "b", serde_json::json!({})),
            ]);
        let graph = plan.to_graph();
        assert_eq!(graph.node_count(), 2);
        assert_eq!(graph.edge_count(), 0);
    }

    #[test]
    fn test_orchestration_plan_to_graph_hierarchical() {
        let root = PlanNode::new(PlanStep::new("r", "t", serde_json::json!({})))
            .with_children(vec![
                PlanNode::new(PlanStep::new("c1", "t", serde_json::json!({}))),
                PlanNode::new(PlanStep::new("c2", "t", serde_json::json!({}))),
            ]);
        let plan = OrchestrationPlan::new(OrchestrationMode::Hierarchical, "s1").with_root(root);
        let graph = plan.to_graph();
        assert_eq!(graph.node_count(), 3);
        assert_eq!(graph.edge_count(), 2);
        assert_eq!(graph.root_id.as_deref(), Some("r"));
    }

    #[test]
    fn test_inject_prev_full_replacement() {
        let args = serde_json::json!({"$prev": true});
        let prev = serde_json::json!({"x": 1});
        let injected = inject_prev(&args, Some(&prev));
        assert_eq!(injected, prev);
    }

    #[test]
    fn test_inject_prev_no_prev_returns_null() {
        let args = serde_json::json!({"$prev": true});
        let injected = inject_prev(&args, None);
        assert_eq!(injected, serde_json::Value::Null);
    }

    #[test]
    fn test_inject_prev_recursive_in_object() {
        let args = serde_json::json!({"a": {"$prev": true}, "b": 2});
        let prev = serde_json::json!({"x": 1});
        let injected = inject_prev(&args, Some(&prev));
        assert_eq!(injected["a"], prev);
        assert_eq!(injected["b"], 2);
    }

    #[test]
    fn test_inject_prev_in_array() {
        let args = serde_json::json!([{"$prev": true}, 2]);
        let prev = serde_json::json!("hello");
        let injected = inject_prev(&args, Some(&prev));
        assert_eq!(injected[0], prev);
        assert_eq!(injected[1], 2);
    }

    #[test]
    fn test_inject_prev_no_placeholder_unchanged() {
        let args = serde_json::json!({"a": 1, "b": "x"});
        let injected = inject_prev(&args, Some(&serde_json::json!({"y": 1})));
        assert_eq!(injected, args);
    }

    #[test]
    fn test_deterministic_planner_translate() {
        let p = DeterministicPlanner.plan("please translate this text", "s1");
        assert_eq!(p.mode, OrchestrationMode::Sequential);
        assert_eq!(p.step_count(), 3);
        assert_eq!(p.steps[0].tool_name, "extract");
        assert_eq!(p.steps[2].tool_name, "format");
    }

    #[test]
    fn test_deterministic_planner_summarize() {
        let p = DeterministicPlanner.plan("summarize the document", "s1");
        assert_eq!(p.mode, OrchestrationMode::Sequential);
        assert_eq!(p.step_count(), 2);
        assert_eq!(p.steps[1].tool_name, "summarize");
    }

    #[test]
    fn test_deterministic_planner_search() {
        let p = DeterministicPlanner.plan("search the web", "s1");
        assert_eq!(p.mode, OrchestrationMode::Parallel);
        assert_eq!(p.step_count(), 2);
    }

    #[test]
    fn test_deterministic_planner_classify() {
        let p = DeterministicPlanner.plan("classify this item", "s1");
        assert_eq!(p.mode, OrchestrationMode::Hierarchical);
        assert!(p.root.is_some());
        let root = p.root.unwrap();
        assert_eq!(root.step.tool_name, "classify");
        assert_eq!(root.child_count(), 2);
    }

    #[test]
    fn test_deterministic_planner_default() {
        let p = DeterministicPlanner.plan("do something unspecified", "s1");
        assert_eq!(p.mode, OrchestrationMode::Sequential);
        assert_eq!(p.step_count(), 1);
        assert_eq!(p.steps[0].tool_name, "default");
    }

    #[test]
    fn test_orchestrator_execute_sequential() {
        let (orch, _) = make_orchestrator_with_tools();
        let plan = DeterministicPlanner.plan("translate this", "s1");
        let summary = orch.execute(&plan).unwrap();
        assert!(summary.success);
        assert_eq!(summary.step_results.len(), 3);
        // 最终输出为最后一步 format 的结果
        assert!(summary.final_output.is_some());
        let final_out = summary.final_output.unwrap();
        assert!(final_out.get("formatted").is_some());
    }

    #[test]
    fn test_orchestrator_execute_parallel() {
        let (orch, _) = make_orchestrator_with_tools();
        let plan = DeterministicPlanner.plan("search it", "s1");
        let summary = orch.execute(&plan).unwrap();
        assert!(summary.success);
        assert_eq!(summary.step_results.len(), 2);
        let gathered = summary.final_output.unwrap();
        assert!(gathered.get("s1").is_some());
        assert!(gathered.get("s2").is_some());
    }

    #[test]
    fn test_orchestrator_execute_hierarchical() {
        let (orch, _) = make_orchestrator_with_tools();
        let plan = DeterministicPlanner.plan("classify this", "s1");
        let summary = orch.execute(&plan).unwrap();
        assert!(summary.success);
        // root + 2 children = 3 results
        assert_eq!(summary.step_results.len(), 3);
        // 根节点结果作为最终输出
        assert!(summary.final_output.is_some());
    }

    #[test]
    fn test_orchestrator_hierarchical_without_root_fails() {
        let (orch, _) = make_orchestrator_with_tools();
        let plan = OrchestrationPlan::new(OrchestrationMode::Hierarchical, "s1");
        let err = orch.execute(&plan).unwrap_err();
        assert!(matches!(err, crate::Error::InvalidInput(_)));
    }

    #[test]
    fn test_orchestrator_sequential_failure_aborts_chain() {
        let (orch, _) = make_orchestrator_with_tools();
        let plan = OrchestrationPlan::new(OrchestrationMode::Sequential, "s1").with_steps(vec![
            PlanStep::new("s1", "fail", serde_json::json!({})),
            PlanStep::new("s2", "echo", serde_json::json!({})).with_depends_on(vec!["s1".into()]),
        ]);
        let summary = orch.execute(&plan).unwrap();
        assert!(!summary.success);
        // 链中断后只应执行第一步
        assert_eq!(summary.step_results.len(), 1);
        assert!(summary.final_output.is_none());
    }

    #[test]
    fn test_orchestrator_hierarchical_node_failure_skips_children() {
        let (orch, _) = make_orchestrator_with_tools();
        let root = PlanNode::new(PlanStep::new("r", "fail", serde_json::json!({})))
            .with_children(vec![PlanNode::new(PlanStep::new(
                "c1",
                "echo",
                serde_json::json!({}),
            ))]);
        let plan = OrchestrationPlan::new(OrchestrationMode::Hierarchical, "s1").with_root(root);
        let summary = orch.execute(&plan).unwrap();
        assert!(!summary.success);
        // 失败的根节点 + 跳过的子节点 → 仅 1 个结果
        assert_eq!(summary.step_results.len(), 1);
    }

    #[test]
    fn test_orchestrator_history_recorded() {
        let (orch, _) = make_orchestrator_with_tools();
        let plan = DeterministicPlanner.plan("search it", "s1");
        orch.execute(&plan).unwrap();
        orch.execute(&plan).unwrap();
        let history = orch.history();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].mode, OrchestrationMode::Parallel);
    }

    #[test]
    fn test_orchestrator_registry_accessor() {
        let (orch, registry) = make_orchestrator_with_tools();
        let r = orch.registry();
        // 同一 Arc
        assert!(Arc::ptr_eq(r, &registry));
        assert!(r.tool_count() > 0);
    }

    #[test]
    fn test_orchestration_summary_success_flag() {
        let summary = OrchestrationSummary {
            mode: OrchestrationMode::Sequential,
            session_id: "s1".into(),
            step_results: vec![],
            final_output: None,
            total_latency_ms: 10,
            success: true,
        };
        assert!(summary.success);
    }

    #[test]
    fn test_plan_graph_node_edge_counts() {
        let mut g = PlanGraph::new(OrchestrationMode::Parallel);
        g.nodes.push(PlanStep::new("s1", "t", serde_json::json!({})));
        g.edges.push(PlanEdge::new("s1", "s2"));
        assert_eq!(g.node_count(), 1);
        assert_eq!(g.edge_count(), 1);
    }

    #[test]
    fn test_deterministic_planner_default_impl() {
        let p = DeterministicPlanner::default();
        let plan = p.plan("classify the document", "s1");
        assert_eq!(plan.mode, OrchestrationMode::Hierarchical);
    }
}
