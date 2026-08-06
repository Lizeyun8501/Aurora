//! 测试质量保障 (Testing & Quality Assurance)
//!
//! Phase 6 / PART VI — 运维可观测性支柱。
//!
//! # 子任务
//! - **6.3.1 单元测试与集成测试**：`cargo test` + vitest + Mock L1 + proptest 属性测试。
//! - **6.3.2 CRDT 一致性测试**：猴子测试随机操作序列 + delta-debugging 反例压缩 + 24 小时稳定性。
//! - **6.3.3 E2E 测试**：Playwright 跨端 + 关键场景覆盖 + 视觉回归截图对比 + 崩溃恢复测试。
//! - **6.3.4 性能基线与回归**：criterion.rs 基线 + CI 10% 退化阈值 + k6 加载测试 + 内存分析。
//! - **6.3.5 覆盖率管控**：tarpaulin Rust ≥70%/核心 ≥80% + vitest TS ≥60% + Codecov PR 评论 + 豁免机制。

use std::collections::HashMap;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::{Error, Result};

// ===========================================================================
// SubTask 6.3.1: 单元测试与集成测试框架
// ===========================================================================

/// 测试结果状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TestStatus {
    Passed,
    Failed,
    Skipped,
    Error,
}

impl TestStatus {
    pub fn label(&self) -> &'static str {
        match self {
            TestStatus::Passed => "passed",
            TestStatus::Failed => "failed",
            TestStatus::Skipped => "skipped",
            TestStatus::Error => "error",
        }
    }

    pub fn is_success(&self) -> bool {
        matches!(self, TestStatus::Passed)
    }
}

/// 单条测试用例结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCaseResult {
    /// 测试名称（如 `tests::content_editor::test_block_insert`）。
    pub name: String,
    /// 所属模块。
    pub module: String,
    /// 测试状态。
    pub status: TestStatus,
    /// 耗时（毫秒）。
    pub duration_ms: u64,
    /// 失败消息。
    pub message: Option<String>,
    /// 执行时间。
    pub timestamp: DateTime<Utc>,
}

/// 测试套件（分组）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestSuite {
    /// 套件名称。
    pub name: String,
    /// 所属包（Rust crate / TS package）。
    pub package: String,
    /// 测试语言。
    pub language: TestLanguage,
    /// 所有测试结果。
    pub cases: Vec<TestCaseResult>,
    /// 执行时间。
    pub executed_at: DateTime<Utc>,
    /// 总耗时（毫秒）。
    pub total_duration_ms: u64,
}

impl TestSuite {
    /// 通过数。
    pub fn passed(&self) -> usize {
        self.cases.iter().filter(|c| c.status == TestStatus::Passed).count()
    }

    /// 失败数。
    pub fn failed(&self) -> usize {
        self.cases.iter().filter(|c| c.status == TestStatus::Failed).count()
    }

    /// 跳过数。
    pub fn skipped(&self) -> usize {
        self.cases.iter().filter(|c| c.status == TestStatus::Skipped).count()
    }

    /// 错误数。
    pub fn errors(&self) -> usize {
        self.cases.iter().filter(|c| c.status == TestStatus::Error).count()
    }

    /// 总数。
    pub fn total(&self) -> usize {
        self.cases.len()
    }

    /// 是否全部通过。
    pub fn all_passed(&self) -> bool {
        self.failed() == 0 && self.errors() == 0
    }

    /// 通过率（0.0 ~ 1.0）。
    pub fn pass_rate(&self) -> f64 {
        if self.total() == 0 {
            return 1.0;
        }
        self.passed() as f64 / self.total() as f64
    }
}

/// 测试语言。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TestLanguage {
    Rust,
    TypeScript,
}

/// 测试运行报告。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestReport {
    /// 报告 ID。
    pub id: String,
    /// 所有套件。
    pub suites: Vec<TestSuite>,
    /// 总测试数。
    pub total: usize,
    /// 总通过数。
    pub passed: usize,
    /// 总失败数。
    pub failed: usize,
    /// 总耗时（毫秒）。
    pub total_duration_ms: u64,
    /// 生成时间。
    pub generated_at: DateTime<Utc>,
    /// Git commit SHA（若有）。
    pub commit: Option<String>,
    /// CI 运行 ID（若有）。
    pub ci_run_id: Option<String>,
}

impl TestReport {
    /// 汇总多个套件。
    pub fn from_suites(suites: Vec<TestSuite>) -> Self {
        let total: usize = suites.iter().map(|s| s.total()).sum();
        let passed: usize = suites.iter().map(|s| s.passed()).sum();
        let failed: usize = suites.iter().map(|s| s.failed()).sum();
        let total_duration_ms: u64 = suites.iter().map(|s| s.total_duration_ms).sum();

        Self {
            id: Uuid::new_v4().to_string(),
            suites,
            total,
            passed,
            failed,
            total_duration_ms,
            generated_at: Utc::now(),
            commit: None,
            ci_run_id: None,
        }
    }

    /// 整体通过率。
    pub fn pass_rate(&self) -> f64 {
        if self.total == 0 {
            return 1.0;
        }
        self.passed as f64 / self.total as f64
    }

    /// 是否全部通过。
    pub fn all_passed(&self) -> bool {
        self.failed == 0
    }

    /// 导出为 JUnit XML（兼容 CI 工具）。
    pub fn to_junit_xml(&self) -> String {
        let mut xml = String::from(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
        xml.push_str(&format!(
            r#"<testsuites name="aurora" tests="{}" failures="{}" time="{:.3}">"#,
            self.total,
            self.failed,
            self.total_duration_ms as f64 / 1000.0
        ));
        for suite in &self.suites {
            xml.push_str(&format!(
                r#"<testsuite name="{}" package="{}" tests="{}" failures="{}" errors="{}" skipped="{}" time="{:.3}">"#,
                suite.name,
                suite.package,
                suite.total(),
                suite.failed(),
                suite.errors(),
                suite.skipped(),
                suite.total_duration_ms as f64 / 1000.0
            ));
            for case in &suite.cases {
                xml.push_str(&format!(
                    r#"<testcase name="{}" classname="{}" time="{:.3}">"#,
                    case.name,
                    suite.name,
                    case.duration_ms as f64 / 1000.0
                ));
                if case.status == TestStatus::Failed || case.status == TestStatus::Error {
                    xml.push_str(&format!(
                        r#"<failure message="{}">{}</failure>"#,
                        case.message.as_deref().unwrap_or(""),
                        case.message.as_deref().unwrap_or("")
                    ));
                } else if case.status == TestStatus::Skipped {
                    xml.push_str("<skipped/>");
                }
                xml.push_str("</testcase>");
            }
            xml.push_str("</testsuite>");
        }
        xml.push_str("</testsuites>");
        xml
    }

    /// 导出为 JSON（供 Codecov / CI 消费）。
    pub fn to_json_report(&self) -> Result<String> {
        serde_json::to_string_pretty(self).map_err(Error::Serialization)
    }
}

// ===========================================================================
// SubTask 6.3.2: CRDT 一致性测试 (CRDT Consistency Testing)
// ===========================================================================

/// CRDT 操作类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CrdtOp {
    /// 插入文本。
    Insert,
    /// 删除文本。
    Delete,
    /// 块级操作：创建块。
    CreateBlock,
    /// 块级操作：删除块。
    DeleteBlock,
    /// 移动块。
    MoveBlock,
    /// 更新块属性。
    UpdateProperty,
}

impl CrdtOp {
    /// 所有操作类型的列表（供随机选择）。
    pub fn all() -> &'static [CrdtOp] {
        &[
            CrdtOp::Insert,
            CrdtOp::Delete,
            CrdtOp::CreateBlock,
            CrdtOp::DeleteBlock,
            CrdtOp::MoveBlock,
            CrdtOp::UpdateProperty,
        ]
    }
}

/// 单次 CRDT 操作记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrdtOperation {
    /// 操作 ID。
    pub id: String,
    /// 操作类型。
    pub op_type: CrdtOp,
    /// 副本 ID（模拟多副本环境）。
    pub replica_id: u32,
    /// 块 ID。
    pub block_id: String,
    /// 操作位置。
    pub position: u32,
    /// 操作长度（Insert/Delete 时有效）。
    pub length: u32,
    /// 操作内容（Insert 时有效）。
    pub content: Option<String>,
    /// 操作时间戳（逻辑时钟）。
    pub lamport_ts: u64,
}

/// CRDT 一致性检查结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrdtConsistencyReport {
    /// 测试 ID。
    pub id: String,
    /// 总操作数。
    pub total_ops: usize,
    /// 副本数。
    pub replica_count: u32,
    /// 是否一致。
    pub consistent: bool,
    /// 不一致的副本对（replica_a, replica_b, 描述）。
    pub divergences: Vec<(u32, u32, String)>,
    /// 测试耗时（毫秒）。
    pub duration_ms: u64,
    /// 使用的反例压缩技术。
    pub reduction_method: Option<String>,
    /// 最小反例操作数（delta-debugging 压缩后）。
    pub min_counter_example_ops: Option<usize>,
}

/// 猴子测试配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonkeyTestConfig {
    /// 副本数（≥2）。
    pub replicas: u32,
    /// 操作总数。
    pub total_ops: usize,
    /// 是否启用随机种子。
    pub seed: Option<u64>,
    /// 是否启用 delta-debugging 反例压缩。
    pub enable_reduction: bool,
    /// 稳定性测试时长（秒），0 表示不执行。
    pub stability_duration_secs: u64,
}

impl Default for MonkeyTestConfig {
    fn default() -> Self {
        Self {
            replicas: 3,
            total_ops: 1000,
            seed: Some(42),
            enable_reduction: true,
            stability_duration_secs: 0,
        }
    }
}

/// CRDT 状态快照：用于多副本比较。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrdtStateSnapshot {
    /// 副本 ID。
    pub replica_id: u32,
    /// 块 ID → 内容 映射。
    pub blocks: HashMap<String, String>,
    /// 块 ID → 属性 映射。
    pub properties: HashMap<String, HashMap<String, String>>,
    /// 块顺序列表。
    pub block_order: Vec<String>,
    /// Lamport 时钟。
    pub lamport_ts: u64,
}

/// CRDT 猴子测试引擎：生成随机操作序列、模拟多副本、检查收敛性。
pub struct CrdtMonkeyTester {
    config: MonkeyTestConfig,
}

impl CrdtMonkeyTester {
    pub fn new(config: MonkeyTestConfig) -> Self {
        Self { config }
    }

    /// 运行一次猴子测试：生成随机操作序列，分发到各副本，检查最终一致性。
    /// `apply_fn` 为将操作应用到副本的回调（由上层提供实际 CRDT 引擎）。
    pub fn run<F>(
        &self,
        mut apply_fn: F,
    ) -> CrdtConsistencyReport
    where
        F: FnMut(CrdtOperation) -> CrdtStateSnapshot,
    {
        let start = Instant::now();
        let ops = self.generate_random_ops();
        let mut replicas: Vec<Vec<CrdtOperation>> =
            vec![Vec::new(); self.config.replicas as usize];

        // 随机分发：每个操作随机发送给一个或多个副本（模拟网络分区/乱序）
        for op in &ops {
            let target = (op.lamport_ts as usize) % self.config.replicas as usize;
            replicas[target].push(op.clone());
        }

        // 各副本独立应用操作序列
        let mut snapshots = Vec::new();
        for replica_ops in replicas.iter() {
            let mut last_snapshot: Option<CrdtStateSnapshot> = None;
            for op in replica_ops {
                last_snapshot = Some(apply_fn(op.clone()));
            }
            if let Some(snap) = last_snapshot {
                snapshots.push(snap);
            }
        }

        // 比较所有副本的一致性
        let mut divergences = Vec::new();
        for i in 0..snapshots.len() {
            for j in (i + 1)..snapshots.len() {
                let diff = compare_snapshots(&snapshots[i], &snapshots[j]);
                if let Some(d) = diff {
                    divergences.push((snapshots[i].replica_id, snapshots[j].replica_id, d));
                }
            }
        }

        let consistent = divergences.is_empty();
        let duration_ms = start.elapsed().as_millis() as u64;

        // delta-debugging 反例压缩
        let (min_ops, reduction_method) = if !consistent && self.config.enable_reduction {
            let reduced = delta_debugging(&ops, &mut apply_fn);
            let method = format!(
                "delta-debugging: {}→{} ops",
                ops.len(),
                reduced
            );
            (Some(reduced), Some(method))
        } else {
            (None, None)
        };

        CrdtConsistencyReport {
            id: Uuid::new_v4().to_string(),
            total_ops: ops.len(),
            replica_count: self.config.replicas,
            consistent,
            divergences,
            duration_ms,
            reduction_method,
            min_counter_example_ops: min_ops,
        }
    }

    /// 运行稳定性测试：持续指定时间，反复随机操作后检查一致性。
    pub fn run_stability_test<F>(
        &self,
        apply_fn: F,
    ) -> Vec<CrdtConsistencyReport>
    where
        F: Fn(CrdtOperation) -> CrdtStateSnapshot + Clone + Send + 'static,
    {
        let deadline = Instant::now()
            + Duration::from_secs(self.config.stability_duration_secs);
        let mut reports = Vec::new();
        let mut round = 0;

        while Instant::now() < deadline {
            let report = self.run(apply_fn.clone());
            round += 1;
            debug!(round, consistent = report.consistent, "stability test round");
            if !report.consistent {
                warn!(round, "CRDT stability test: divergence detected");
            }
            reports.push(report);
        }

        info!(rounds = round, "stability test completed");
        reports
    }

    /// 生成随机操作序列。
    fn generate_random_ops(&self) -> Vec<CrdtOperation> {
        let seed = self.config.seed.unwrap_or(42);
        let mut rng = SimpleRng::new(seed);

        let all_ops = CrdtOp::all();
        let mut ops = Vec::with_capacity(self.config.total_ops);

        for i in 0..self.config.total_ops {
            let op_type = all_ops[rng.next() as usize % all_ops.len()];
            let block_id = format!("block-{}", rng.next() as u32 % 50);
            let content = if matches!(op_type, CrdtOp::Insert) {
                Some(format!("text-{}", rng.next() as u32 % 1000))
            } else {
                None
            };

            ops.push(CrdtOperation {
                id: Uuid::new_v4().to_string(),
                op_type,
                replica_id: rng.next() as u32 % self.config.replicas,
                block_id,
                position: rng.next() as u32 % 100,
                length: 1 + (rng.next() as u32 % 10),
                content,
                lamport_ts: i as u64,
            });
        }

        ops
    }
}

/// 比较两个副本快照，返回不一致描述。
fn compare_snapshots(a: &CrdtStateSnapshot, b: &CrdtStateSnapshot) -> Option<String> {
    let mut diffs = Vec::new();

    // 检查块顺序
    if a.block_order != b.block_order {
        diffs.push(format!(
            "block_order: {:?} vs {:?}",
            &a.block_order[..a.block_order.len().min(5)],
            &b.block_order[..b.block_order.len().min(5)]
        ));
    }

    // 检查块内容
    let all_keys: std::collections::HashSet<_> =
        a.blocks.keys().chain(b.blocks.keys()).collect();
    for key in all_keys {
        match (a.blocks.get(key), b.blocks.get(key)) {
            (Some(va), Some(vb)) if va != vb => {
                diffs.push(format!("block '{}': content differs", key));
            }
            (Some(_), None) => {
                diffs.push(format!("block '{}': only in replica {}", key, a.replica_id));
            }
            (None, Some(_)) => {
                diffs.push(format!("block '{}': only in replica {}", key, b.replica_id));
            }
            _ => {}
        }
    }

    // 检查属性
    let all_props: std::collections::HashSet<_> =
        a.properties.keys().chain(b.properties.keys()).collect();
    for key in all_props {
        match (a.properties.get(key), b.properties.get(key)) {
            (Some(pa), Some(pb)) if pa != pb => {
                diffs.push(format!("property '{}': differs", key));
            }
            (Some(_), None) => {
                diffs.push(format!("property '{}': only in replica {}", key, a.replica_id));
            }
            (None, Some(_)) => {
                diffs.push(format!("property '{}': only in replica {}", key, b.replica_id));
            }
            _ => {}
        }
    }

    if diffs.is_empty() {
        None
    } else {
        Some(diffs.join("; "))
    }
}

/// Delta-debugging 反例压缩：二分搜索最小反例操作集。
fn delta_debugging<F>(ops: &[CrdtOperation], apply_fn: &mut F) -> usize
where
    F: FnMut(CrdtOperation) -> CrdtStateSnapshot,
{
    // 简化实现：二分搜索找到最小仍不一致的子集大小
    let mut low = 1;
    let mut high = ops.len();

    while low < high {
        let mid = (low + high) / 2;
        let subset = &ops[..mid];

        let mut snapshots = Vec::new();
        for op in subset {
            let snap = apply_fn(op.clone());
            snapshots.push(snap);
        }

        // 简化比较：取第一个和最后一个快照
        if snapshots.len() >= 2 {
            let first = &snapshots[0];
            let last = &snapshots.last().unwrap();
            if compare_snapshots(first, last).is_some() {
                high = mid;
            } else {
                low = mid + 1;
            }
        } else {
            break;
        }
    }

    high
}

/// 简单的线性同余伪随机数生成器。
struct SimpleRng {
    state: u64,
}

impl SimpleRng {
    fn new(seed: u64) -> Self {
        Self { state: seed.wrapping_add(1) } // 避免 seed=0
    }

    fn next(&mut self) -> u64 {
        // LCG: X_{n+1} = (a*X_n + c) mod m
        self.state = self.state.wrapping_mul(6364136223846793005).wrapping_add(1);
        // Xorshift 混洗
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }
}

// ===========================================================================
// SubTask 6.3.3: E2E 测试 (End-to-End Testing)
// ===========================================================================

/// E2E 测试场景。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct E2EScenario {
    /// 场景名称。
    pub name: String,
    /// 场景描述。
    pub description: String,
    /// 涉及的功能模块。
    pub modules: Vec<String>,
    /// 测试步骤数。
    pub steps: Vec<E2EStep>,
    /// 是否关键场景（CI 必须通过）。
    pub critical: bool,
    /// 目标平台。
    pub platforms: Vec<TestPlatform>,
}

/// E2E 测试步骤。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct E2EStep {
    /// 步骤序号。
    pub order: u32,
    /// 动作描述。
    pub action: String,
    /// 期望结果。
    pub expected: String,
    /// 选择器（Playwright 定位器）。
    pub selector: Option<String>,
    /// 输入值。
    pub input: Option<String>,
}

/// 测试平台。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TestPlatform {
    Desktop,
    Mobile,
    Web,
    All,
}

/// E2E 测试结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct E2ETestResult {
    /// 场景名称。
    pub scenario: String,
    /// 是否通过。
    pub passed: bool,
    /// 失败步骤序号。
    pub failed_step: Option<u32>,
    /// 失败原因。
    pub error_message: Option<String>,
    /// 视觉回归差异（截图对比）。
    pub visual_diff_percent: Option<f64>,
    /// 崩溃恢复是否成功。
    pub crash_recovery_ok: Option<bool>,
    /// 执行平台。
    pub platform: TestPlatform,
    /// 耗时（毫秒）。
    pub duration_ms: u64,
    /// 截图路径。
    pub screenshot_path: Option<String>,
}

/// E2E 测试管理器：管理场景定义、执行记录、视觉回归阈值。
pub struct E2ETestManager {
    scenarios: RwLock<Vec<E2EScenario>>,
    results: RwLock<Vec<E2ETestResult>>,
    /// 视觉回归最大允许差异（百分比，默认 0.5%）。
    visual_diff_threshold: f64,
    /// 崩溃恢复超时（毫秒）。
    crash_recovery_timeout_ms: u64,
}

impl E2ETestManager {
    pub fn new() -> Self {
        Self {
            scenarios: RwLock::new(Self::default_scenarios()),
            results: RwLock::new(Vec::new()),
            visual_diff_threshold: 0.5,
            crash_recovery_timeout_ms: 10000,
        }
    }

    /// 注册自定义场景。
    pub fn register_scenario(&self, scenario: E2EScenario) {
        self.scenarios.write().push(scenario);
    }

    /// 获取所有场景。
    pub fn scenarios(&self) -> Vec<E2EScenario> {
        self.scenarios.read().clone()
    }

    /// 获取关键场景（CI 必须通过）。
    pub fn critical_scenarios(&self) -> Vec<E2EScenario> {
        self.scenarios
            .read()
            .iter()
            .filter(|s| s.critical)
            .cloned()
            .collect()
    }

    /// 记录测试结果。
    pub fn record_result(&self, result: E2ETestResult) {
        self.results.write().push(result);
    }

    /// 获取所有结果。
    pub fn results(&self) -> Vec<E2ETestResult> {
        self.results.read().clone()
    }

    /// 检查关键场景是否全部通过。
    pub fn critical_all_passed(&self) -> bool {
        let results = self.results.read();
        let critical_names: std::collections::HashSet<_> = self
            .scenarios
            .read()
            .iter()
            .filter(|s| s.critical)
            .map(|s| s.name.clone())
            .collect();

        for name in &critical_names {
            let latest = results
                .iter()
                .rev()
                .find(|r| r.scenario == *name);
            match latest {
                Some(r) if !r.passed => return false,
                None => return false, // 未执行
                _ => {}
            }
        }
        true
    }

    /// 视觉回归阈值。
    pub fn visual_diff_threshold(&self) -> f64 {
        self.visual_diff_threshold
    }

    pub fn set_visual_diff_threshold(&mut self, threshold: f64) {
        self.visual_diff_threshold = threshold;
    }

    /// 预置关键 E2E 场景。
    fn default_scenarios() -> Vec<E2EScenario> {
        vec![
            E2EScenario {
                name: "create_and_edit_note".into(),
                description: "创建笔记、编辑内容、验证自动保存".into(),
                modules: vec!["content-editor".into()],
                steps: vec![
                    E2EStep {
                        order: 1,
                        action: "点击新建笔记按钮".into(),
                        expected: "创建空白笔记页面".into(),
                        selector: Some("[data-testid='new-note-btn']".into()),
                        input: None,
                    },
                    E2EStep {
                        order: 2,
                        action: "在编辑器中输入文本".into(),
                        expected: "文本实时渲染".into(),
                        selector: Some("[data-testid='editor-content']".into()),
                        input: Some("Hello Aurora!".into()),
                    },
                    E2EStep {
                        order: 3,
                        action: "等待 2 秒自动保存".into(),
                        expected: "显示'已保存'状态".into(),
                        selector: Some("[data-testid='save-status']".into()),
                        input: None,
                    },
                ],
                critical: true,
                platforms: vec![TestPlatform::All],
            },
            E2EScenario {
                name: "sync_between_devices".into(),
                description: "两台设备间 CRDT 同步".into(),
                modules: vec!["sync".into(), "content-editor".into()],
                steps: vec![
                    E2EStep {
                        order: 1,
                        action: "设备 A 创建笔记并编辑".into(),
                        expected: "设备 A 显示内容".into(),
                        selector: Some("[data-testid='editor-content']".into()),
                        input: Some("sync test".into()),
                    },
                    E2EStep {
                        order: 2,
                        action: "等待同步完成".into(),
                        expected: "设备 B 显示相同内容".into(),
                        selector: Some("[data-testid='editor-content']".into()),
                        input: None,
                    },
                ],
                critical: true,
                platforms: vec![TestPlatform::Desktop, TestPlatform::Web],
            },
            E2EScenario {
                name: "crash_recovery".into(),
                description: "崩溃后数据恢复".into(),
                modules: vec!["content-editor".into(), "sync".into()],
                steps: vec![
                    E2EStep {
                        order: 1,
                        action: "创建笔记并输入内容".into(),
                        expected: "内容保存成功".into(),
                        selector: Some("[data-testid='editor-content']".into()),
                        input: Some("crash recovery test data".into()),
                    },
                    E2EStep {
                        order: 2,
                        action: "模拟应用崩溃（强制关闭进程）".into(),
                        expected: "应用退出".into(),
                        selector: None,
                        input: None,
                    },
                    E2EStep {
                        order: 3,
                        action: "重新启动应用".into(),
                        expected: "之前的内容完整恢复".into(),
                        selector: Some("[data-testid='editor-content']".into()),
                        input: None,
                    },
                ],
                critical: true,
                platforms: vec![TestPlatform::Desktop],
            },
            E2EScenario {
                name: "import_markdown".into(),
                description: "Markdown 文件导入".into(),
                modules: vec!["import-export".into()],
                steps: vec![
                    E2EStep {
                        order: 1,
                        action: "点击导入按钮".into(),
                        expected: "文件选择对话框打开".into(),
                        selector: Some("[data-testid='import-btn']".into()),
                        input: None,
                    },
                    E2EStep {
                        order: 2,
                        action: "选择 Markdown 文件".into(),
                        expected: "内容正确渲染为块".into(),
                        selector: Some("[data-testid='editor-content']".into()),
                        input: None,
                    },
                ],
                critical: false,
                platforms: vec![TestPlatform::All],
            },
            E2EScenario {
                name: "knowledge_graph_interaction".into(),
                description: "知识图谱交互".into(),
                modules: vec!["knowledge-network".into()],
                steps: vec![
                    E2EStep {
                        order: 1,
                        action: "创建两个有关联的笔记".into(),
                        expected: "双链创建成功".into(),
                        selector: Some("[data-testid='editor-content']".into()),
                        input: Some("[[linked note]]".into()),
                    },
                    E2EStep {
                        order: 2,
                        action: "打开知识图谱视图".into(),
                        expected: "节点和连线正确显示".into(),
                        selector: Some("[data-testid='graph-canvas']".into()),
                        input: None,
                    },
                ],
                critical: false,
                platforms: vec![TestPlatform::Desktop, TestPlatform::Web],
            },
        ]
    }
}

impl Default for E2ETestManager {
    fn default() -> Self {
        Self::new()
    }
}

// ===========================================================================
// SubTask 6.3.4: 性能基线与回归 (Performance Baseline & Regression)
// ===========================================================================

/// 性能基准条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceBaseline {
    /// 基准名称（如 "content_insert_1000_blocks"）。
    pub name: String,
    /// 所属模块。
    pub module: String,
    /// 度量类型。
    pub metric_type: PerformanceMetricType,
    /// 基准值。
    pub baseline_value: f64,
    /// 单位。
    pub unit: String,
    /// 退化阈值（百分比，如 10 表示超过 10% 即告警）。
    pub regression_threshold_percent: f64,
    /// 记录时间。
    pub recorded_at: DateTime<Utc>,
    /// Git commit SHA。
    pub commit: Option<String>,
}

/// 性能度量类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PerformanceMetricType {
    /// 吞吐量（ops/sec）。
    Throughput,
    /// 延迟（ms）。
    Latency,
    /// 内存（MB）。
    Memory,
    /// 启动时间（ms）。
    StartupTime,
    /// 文件大小（bytes）。
    FileSize,
}

/// 单次性能测试结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceTestResult {
    /// 基准名称。
    pub baseline_name: String,
    /// 当前值。
    pub current_value: f64,
    /// 基线值。
    pub baseline_value: f64,
    /// 变化百分比（正=退化）。
    pub change_percent: f64,
    /// 是否超过退化阈值。
    pub regression: bool,
    /// 测试时间。
    pub tested_at: DateTime<Utc>,
    /// 迭代次数。
    pub iterations: u32,
    /// 标准差。
    pub std_dev: Option<f64>,
    /// p99 值。
    pub p99: Option<f64>,
}

/// 性能基准注册表。
pub struct PerformanceRegistry {
    baselines: RwLock<HashMap<String, PerformanceBaseline>>,
    history: RwLock<Vec<PerformanceTestResult>>,
    /// 全局退化阈值（%，默认 10%）。
    default_regression_threshold: f64,
}

impl PerformanceRegistry {
    pub fn new() -> Self {
        Self {
            baselines: RwLock::new(HashMap::new()),
            history: RwLock::new(Vec::new()),
            default_regression_threshold: 10.0,
        }
    }

    /// 注册/更新基准。
    pub fn set_baseline(&self, baseline: PerformanceBaseline) {
        self.baselines.write().insert(baseline.name.clone(), baseline);
    }

    /// 获取基准。
    pub fn get_baseline(&self, name: &str) -> Option<PerformanceBaseline> {
        self.baselines.read().get(name).cloned()
    }

    /// 记录测试结果并与基线对比。
    pub fn record_and_check(
        &self,
        name: &str,
        current_value: f64,
        iterations: u32,
        std_dev: Option<f64>,
        p99: Option<f64>,
    ) -> Result<PerformanceTestResult> {
        let baselines = self.baselines.read();
        let baseline = baselines
            .get(name)
            .ok_or_else(|| Error::InvalidInput(format!("baseline not found: {name}")))?;

        let change_percent = if baseline.baseline_value > 0.0 {
            ((current_value - baseline.baseline_value) / baseline.baseline_value) * 100.0
        } else {
            0.0
        };

        let regression = change_percent > baseline.regression_threshold_percent;
        let result = PerformanceTestResult {
            baseline_name: name.to_string(),
            current_value,
            baseline_value: baseline.baseline_value,
            change_percent,
            regression,
            tested_at: Utc::now(),
            iterations,
            std_dev,
            p99,
        };

        self.history.write().push(result.clone());

        if regression {
            warn!(
                baseline = name,
                change_pct = change_percent,
                threshold = baseline.regression_threshold_percent,
                "performance regression detected"
            );
        }

        Ok(result)
    }

    /// 获取最近 N 次结果。
    pub fn recent_results(&self, n: usize) -> Vec<PerformanceTestResult> {
        let history = self.history.read();
        history.iter().rev().take(n).cloned().collect()
    }

    /// 检查所有基准是否有退化。
    pub fn check_all_regressions(&self) -> Vec<PerformanceTestResult> {
        self.history
            .read()
            .iter()
            .filter(|r| r.regression)
            .cloned()
            .collect()
    }

    /// 导出为 criterion.rs 兼容格式。
    pub fn to_criterion_report(&self) -> Result<String> {
        let baselines = self.baselines.read();
        let mut report = String::from("# Aurora Performance Baselines\n\n");
        for (name, bl) in baselines.iter() {
            report.push_str(&format!(
                "| {} | {} | {:.2} {} | {:.1}% |\n",
                name, bl.module, bl.baseline_value, bl.unit, bl.regression_threshold_percent
            ));
        }
        Ok(report)
    }
}

impl Default for PerformanceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ===========================================================================
// SubTask 6.3.5: 覆盖率管控 (Coverage Governance)
// ===========================================================================

/// 覆盖率统计。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageStats {
    /// 模块/包名。
    pub package: String,
    /// 语言。
    pub language: TestLanguage,
    /// 行覆盖率（%）。
    pub line_coverage: f64,
    /// 分支覆盖率（%）。
    pub branch_coverage: Option<f64>,
    /// 函数覆盖率（%）。
    pub function_coverage: Option<f64>,
    /// 总行数。
    pub total_lines: usize,
    /// 覆盖行数。
    pub covered_lines: usize,
    /// 是否通过门禁。
    pub gate_passed: bool,
    /// 统计时间。
    pub timestamp: DateTime<Utc>,
}

/// 覆盖率门禁配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageGate {
    /// Rust 整体最低覆盖率（%）。
    pub rust_min_total: f64,
    /// Rust 核心模块最低覆盖率（%）。
    pub rust_min_core: f64,
    /// TypeScript 最低覆盖率（%）。
    pub ts_min_total: f64,
    /// 是否在 PR 中评论。
    pub comment_on_pr: bool,
    /// 豁免模块列表（不检查覆盖率）。
    pub exempt_modules: Vec<String>,
}

impl Default for CoverageGate {
    fn default() -> Self {
        Self {
            rust_min_total: 70.0,
            rust_min_core: 80.0,
            ts_min_total: 60.0,
            comment_on_pr: true,
            exempt_modules: vec![],
        }
    }
}

/// 覆盖率管理器。
pub struct CoverageManager {
    gate: CoverageGate,
    stats: RwLock<Vec<CoverageStats>>,
}

impl CoverageManager {
    pub fn new(gate: CoverageGate) -> Self {
        Self {
            gate,
            stats: RwLock::new(Vec::new()),
        }
    }

    /// 记录覆盖率统计。
    pub fn record(&self, stats: CoverageStats) {
        let mut all = self.stats.write();
        // 替换同包名的最新记录
        all.retain(|s| s.package != stats.package);
        all.push(stats);
    }

    /// 检查是否所有包都通过门禁。
    pub fn check_gates(&self) -> (bool, Vec<String>) {
        let stats = self.stats.read();
        let mut failures = Vec::new();

        for stat in stats.iter() {
            let is_exempt = self.gate.exempt_modules.contains(&stat.package);

            if is_exempt {
                continue;
            }

            let is_core = stat.package.contains("aurora-core");

            let threshold = match stat.language {
                TestLanguage::Rust if is_core => self.gate.rust_min_core,
                TestLanguage::Rust => self.gate.rust_min_total,
                TestLanguage::TypeScript => self.gate.ts_min_total,
            };

            if stat.line_coverage < threshold {
                failures.push(format!(
                    "{}: {:.1}% < {:.1}% ({} core={})",
                    stat.package,
                    stat.line_coverage,
                    threshold,
                    stat.language == TestLanguage::Rust,
                    is_core
                ));
            }
        }

        (failures.is_empty(), failures)
    }

    /// 生成 Codecov PR 评论内容。
    pub fn generate_pr_comment(&self) -> String {
        if !self.gate.comment_on_pr {
            return String::new();
        }

        let stats = self.stats.read();
        let (gate_ok, failures) = self.check_gates();

        let mut comment = String::from("## Coverage Report\n\n");
        comment.push_str("| Package | Language | Line % | Branch % | Gate |\n");
        comment.push_str("|---------|----------|--------|----------|------|\n");

        for stat in stats.iter() {
            let gate_icon = if stat.gate_passed { "✅" } else { "❌" };
            comment.push_str(&format!(
                "| {} | {:?} | {:.1}% | {} | {} |\n",
                stat.package,
                stat.language,
                stat.line_coverage,
                stat.branch_coverage
                    .map(|b| format!("{:.1}%", b))
                    .unwrap_or_else(|| "N/A".into()),
                gate_icon
            ));
        }

        if !gate_ok {
            comment.push_str("\n### ⚠️ Coverage Gate Failed\n");
            for f in &failures {
                comment.push_str(&format!("- {}\n", f));
            }
        } else {
            comment.push_str("\n### ✅ All coverage gates passed\n");
        }

        comment
    }

    /// 获取门禁配置。
    pub fn gate(&self) -> &CoverageGate {
        &self.gate
    }

    /// 获取所有统计。
    pub fn stats(&self) -> Vec<CoverageStats> {
        self.stats.read().clone()
    }

    /// 导出为 Codecov YAML 片段。
    pub fn to_codecov_yaml(&self) -> String {
        let mut yaml = String::from("coverage:\n");
        yaml.push_str("  status:\n");
        yaml.push_str("    project:\n");
        yaml.push_str(&format!(
            "      default:\n        target: {:.0}%\n",
            self.gate.rust_min_total
        ));
        yaml.push_str("    patch: off\n");
        yaml.push_str("  comment:\n");
        yaml.push_str(&format!(
            "    behavior: {}\n",
            if self.gate.comment_on_pr {
                "new"
            } else {
                "off"
            }
        ));
        yaml
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Test Suite & Report ----

    #[test]
    fn test_suite_basic_stats() {
        let suite = TestSuite {
            name: "unit_tests".into(),
            package: "aurora-core".into(),
            language: TestLanguage::Rust,
            cases: vec![
                TestCaseResult {
                    name: "test_a".into(),
                    module: "mod1".into(),
                    status: TestStatus::Passed,
                    duration_ms: 10,
                    message: None,
                    timestamp: Utc::now(),
                },
                TestCaseResult {
                    name: "test_b".into(),
                    module: "mod1".into(),
                    status: TestStatus::Failed,
                    duration_ms: 5,
                    message: Some("assertion failed".into()),
                    timestamp: Utc::now(),
                },
                TestCaseResult {
                    name: "test_c".into(),
                    module: "mod2".into(),
                    status: TestStatus::Skipped,
                    duration_ms: 0,
                    message: None,
                    timestamp: Utc::now(),
                },
            ],
            executed_at: Utc::now(),
            total_duration_ms: 15,
        };

        assert_eq!(suite.total(), 3);
        assert_eq!(suite.passed(), 1);
        assert_eq!(suite.failed(), 1);
        assert_eq!(suite.skipped(), 1);
        assert!(!suite.all_passed());
        assert!((suite.pass_rate() - 1.0 / 3.0).abs() < 0.01);
    }

    #[test]
    fn test_report_junit_xml() {
        let suite = TestSuite {
            name: "suite1".into(),
            package: "aurora-core".into(),
            language: TestLanguage::Rust,
            cases: vec![TestCaseResult {
                name: "test_x".into(),
                module: "m".into(),
                status: TestStatus::Passed,
                duration_ms: 1,
                message: None,
                timestamp: Utc::now(),
            }],
            executed_at: Utc::now(),
            total_duration_ms: 1,
        };
        let report = TestReport::from_suites(vec![suite]);
        let xml = report.to_junit_xml();
        assert!(xml.contains("<testsuites"));
        assert!(xml.contains("aurora"));
        assert!(xml.contains("test_x"));
        assert!(xml.contains("</testsuites>"));
    }

    #[test]
    fn test_report_json() {
        let suite = TestSuite {
            name: "s".into(),
            package: "p".into(),
            language: TestLanguage::Rust,
            cases: vec![],
            executed_at: Utc::now(),
            total_duration_ms: 0,
        };
        let report = TestReport::from_suites(vec![suite]);
        let json = report.to_json_report().unwrap();
        assert!(json.contains("\"passed\""));
        assert!(json.contains("\"total\""));
    }

    // ---- CRDT Monkey Testing ----

    #[test]
    fn crdt_monkey_test_generates_ops() {
        let config = MonkeyTestConfig {
            replicas: 3,
            total_ops: 100,
            seed: Some(1),
            enable_reduction: false,
            stability_duration_secs: 0,
        };
        let tester = CrdtMonkeyTester::new(config);

        // 使用简单应用函数：始终返回空状态
        let report = tester.run(|_op| CrdtStateSnapshot {
            replica_id: 0,
            blocks: HashMap::new(),
            properties: HashMap::new(),
            block_order: vec![],
            lamport_ts: 0,
        });

        assert_eq!(report.total_ops, 100);
        assert_eq!(report.replica_count, 3);
        assert!(report.consistent); // 空状态一定一致
    }

    #[test]
    fn crdt_monkey_test_detects_divergence() {
        let config = MonkeyTestConfig {
            replicas: 2,
            total_ops: 50,
            seed: Some(2),
            enable_reduction: true,
            stability_duration_secs: 0,
        };
        let tester = CrdtMonkeyTester::new(config);

        let mut counter = 0;
        let report = tester.run(move |op| {
            counter += 1;
            // 故意让不同副本产生不同内容
            let content = format!("replica-{}-call-{}", op.replica_id, counter);
            let mut blocks = HashMap::new();
            blocks.insert(op.block_id.clone(), content);
            let mut order = vec![op.block_id.clone()];
            // 引入不确定性
            if op.replica_id == 1 {
                order.push("extra-block".into());
            }
            CrdtStateSnapshot {
                replica_id: op.replica_id,
                blocks,
                properties: HashMap::new(),
                block_order: order,
                lamport_ts: op.lamport_ts,
            }
        });

        // 由于故意引入了 divergence，应该检测到不一致
        assert!(!report.consistent);
        assert!(!report.divergences.is_empty());
    }

    #[test]
    fn simple_rng_deterministic() {
        let mut rng1 = SimpleRng::new(42);
        let mut rng2 = SimpleRng::new(42);
        for _ in 0..100 {
            assert_eq!(rng1.next(), rng2.next());
        }
    }

    #[test]
    fn compare_snapshots_detects_differences() {
        let mut a = CrdtStateSnapshot {
            replica_id: 1,
            blocks: HashMap::from([("b1".into(), "hello".into())]),
            properties: HashMap::new(),
            block_order: vec!["b1".into()],
            lamport_ts: 1,
        };
        let mut b = CrdtStateSnapshot {
            replica_id: 2,
            blocks: HashMap::from([("b1".into(), "world".into())]),
            properties: HashMap::new(),
            block_order: vec!["b1".into()],
            lamport_ts: 1,
        };

        let diff = compare_snapshots(&a, &b);
        assert!(diff.is_some());
        assert!(diff.unwrap().contains("content differs"));
    }

    #[test]
    fn compare_snapshots_identical() {
        let a = CrdtStateSnapshot {
            replica_id: 1,
            blocks: HashMap::from([("b1".into(), "same".into())]),
            properties: HashMap::new(),
            block_order: vec!["b1".into()],
            lamport_ts: 1,
        };
        let b = a.clone();
        assert!(compare_snapshots(&a, &b).is_none());
    }

    // ---- E2E Testing ----

    #[test]
    fn e2e_manager_has_default_scenarios() {
        let mgr = E2ETestManager::new();
        let scenarios = mgr.scenarios();
        assert!(scenarios.len() >= 5);
    }

    #[test]
    fn e2e_manager_critical_scenarios() {
        let mgr = E2ETestManager::new();
        let critical = mgr.critical_scenarios();
        assert!(critical.iter().all(|s| s.critical));
        // create_and_edit_note, sync_between_devices, crash_recovery 是关键的
        assert!(critical.len() >= 3);
    }

    #[test]
    fn e2e_manager_record_and_check() {
        let mgr = E2ETestManager::new();
        mgr.record_result(E2ETestResult {
            scenario: "create_and_edit_note".into(),
            passed: true,
            failed_step: None,
            error_message: None,
            visual_diff_percent: Some(0.1),
            crash_recovery_ok: None,
            platform: TestPlatform::Desktop,
            duration_ms: 500,
            screenshot_path: None,
        });
        mgr.record_result(E2ETestResult {
            scenario: "sync_between_devices".into(),
            passed: true,
            failed_step: None,
            error_message: None,
            visual_diff_percent: None,
            crash_recovery_ok: None,
            platform: TestPlatform::Web,
            duration_ms: 1200,
            screenshot_path: None,
        });
        mgr.record_result(E2ETestResult {
            scenario: "crash_recovery".into(),
            passed: true,
            failed_step: None,
            error_message: None,
            visual_diff_percent: None,
            crash_recovery_ok: Some(true),
            platform: TestPlatform::Desktop,
            duration_ms: 3000,
            screenshot_path: None,
        });

        assert!(mgr.critical_all_passed());
    }

    #[test]
    fn e2e_manager_critical_failure_detected() {
        let mgr = E2ETestManager::new();
        mgr.record_result(E2ETestResult {
            scenario: "create_and_edit_note".into(),
            passed: false,
            failed_step: Some(2),
            error_message: Some("element not found".into()),
            visual_diff_percent: None,
            crash_recovery_ok: None,
            platform: TestPlatform::Desktop,
            duration_ms: 100,
            screenshot_path: Some("error.png".into()),
        });
        assert!(!mgr.critical_all_passed());
    }

    // ---- Performance Baseline ----

    #[test]
    fn performance_registry_set_and_check() {
        let registry = PerformanceRegistry::new();
        registry.set_baseline(PerformanceBaseline {
            name: "block_insert".into(),
            module: "content-editor".into(),
            metric_type: PerformanceMetricType::Throughput,
            baseline_value: 1000.0,
            unit: "ops/sec".into(),
            regression_threshold_percent: 10.0,
            recorded_at: Utc::now(),
            commit: None,
        });

        // 无退化
        let result = registry
            .record_and_check("block_insert", 1050.0, 10, Some(50.0), Some(1100.0))
            .unwrap();
        assert!(!result.regression);
        assert!((result.change_percent - 5.0).abs() < 0.1);

        // 有退化
        let result2 = registry
            .record_and_check("block_insert", 1200.0, 10, None, None)
            .unwrap();
        assert!(result2.regression);
        assert!((result2.change_percent - 20.0).abs() < 0.1);
    }

    #[test]
    fn performance_registry_unknown_baseline_errors() {
        let registry = PerformanceRegistry::new();
        let err = registry
            .record_and_check("unknown", 100.0, 1, None, None)
            .unwrap_err();
        assert!(matches!(err, Error::InvalidInput(_)));
    }

    #[test]
    fn performance_registry_criterion_report() {
        let registry = PerformanceRegistry::new();
        registry.set_baseline(PerformanceBaseline {
            name: "t1".into(),
            module: "m1".into(),
            metric_type: PerformanceMetricType::Latency,
            baseline_value: 5.0,
            unit: "ms".into(),
            regression_threshold_percent: 10.0,
            recorded_at: Utc::now(),
            commit: None,
        });
        let report = registry.to_criterion_report().unwrap();
        assert!(report.contains("t1"));
        assert!(report.contains("ms"));
    }

    // ---- Coverage Governance ----

    #[test]
    fn coverage_manager_gate_pass() {
        let mgr = CoverageManager::new(CoverageGate::default());
        mgr.record(CoverageStats {
            package: "aurora-core".into(),
            language: TestLanguage::Rust,
            line_coverage: 85.0,
            branch_coverage: Some(80.0),
            function_coverage: Some(90.0),
            total_lines: 5000,
            covered_lines: 4250,
            gate_passed: true,
            timestamp: Utc::now(),
        });
        mgr.record(CoverageStats {
            package: "aurora-ui".into(),
            language: TestLanguage::TypeScript,
            line_coverage: 65.0,
            branch_coverage: Some(55.0),
            function_coverage: Some(70.0),
            total_lines: 3000,
            covered_lines: 1950,
            gate_passed: true,
            timestamp: Utc::now(),
        });

        let (ok, failures) = mgr.check_gates();
        assert!(ok, "gates should pass: {:?}", failures);
    }

    #[test]
    fn coverage_manager_gate_fail() {
        let mgr = CoverageManager::new(CoverageGate::default());
        mgr.record(CoverageStats {
            package: "aurora-core".into(),
            language: TestLanguage::Rust,
            line_coverage: 75.0, // 低于 80%
            branch_coverage: None,
            function_coverage: None,
            total_lines: 100,
            covered_lines: 75,
            gate_passed: false,
            timestamp: Utc::now(),
        });

        let (ok, failures) = mgr.check_gates();
        assert!(!ok);
        assert!(!failures.is_empty());
        assert!(failures[0].contains("aurora-core"));
    }

    #[test]
    fn coverage_manager_exempt_module() {
        let mut gate = CoverageGate::default();
        gate.exempt_modules = vec!["experimental-module".into()];
        let mgr = CoverageManager::new(gate);
        mgr.record(CoverageStats {
            package: "experimental-module".into(),
            language: TestLanguage::Rust,
            line_coverage: 10.0, // 豁免
            branch_coverage: None,
            function_coverage: None,
            total_lines: 100,
            covered_lines: 10,
            gate_passed: true,
            timestamp: Utc::now(),
        });

        let (ok, _) = mgr.check_gates();
        assert!(ok); // 豁免模块不检查
    }

    #[test]
    fn coverage_manager_pr_comment() {
        let mgr = CoverageManager::new(CoverageGate::default());
        mgr.record(CoverageStats {
            package: "aurora-core".into(),
            language: TestLanguage::Rust,
            line_coverage: 85.0,
            branch_coverage: Some(80.0),
            function_coverage: None,
            total_lines: 100,
            covered_lines: 85,
            gate_passed: true,
            timestamp: Utc::now(),
        });

        let comment = mgr.generate_pr_comment();
        assert!(comment.contains("Coverage Report"));
        assert!(comment.contains("aurora-core"));
        assert!(comment.contains("85.0%"));
    }

    #[test]
    fn coverage_manager_codecov_yaml() {
        let mgr = CoverageManager::new(CoverageGate::default());
        let yaml = mgr.to_codecov_yaml();
        assert!(yaml.contains("coverage:"));
        assert!(yaml.contains("target:"));
        assert!(yaml.contains("70%"));
    }
}
