//! 任务投影 — V20 §3.2/§4.5（Phase 1 四投影之三）
//!
//! 消费 `TaskStatusChanged`（Medium 通道）+ `NoteCreated/NoteDeleted`
//! 聚合全库任务读模型 — **TodayView 的数据源**（V20 §5.4.2 架构约束：
//! TodayView 聚合结果由 Rust 侧视图模型产出，前端只做渲染）。
//!
//! 设计:
//! - 任务主存储在 NoteDoc `tasks` 容器（CRDT 同步单元）；
//!   本投影聚合为 `task_id → TaskViewRow` 内存表 + KVStore 水位线
//! - `NoteCreated` 事件携带首任务批次 → 播种（后续 PR: create_note
//!   FFI 发任务事件后自动入列）
//! - `TaskStatusChanged` → 状态迁移（GTD 状态机校验由 NoteDoc 侧负责，
//!   投影幂等应用最终值）
//! - `NoteDeleted` → 该笔记任务整组移除
//!
//! 查询接口（对齐 GTD2.0 四分区 + V19 §13）:
//! - `by_status(status)` — 下一步行动/等待/计划/已完成
//! - `today(due_before_ms)` — 今日到期（含逾期红点）
//! - `stats()` — TodayView 头部统计（进行中/已完成）

use std::collections::BTreeMap;
use std::sync::RwLock;

use async_trait::async_trait;
use tracing::info;

use crate::event_bus::layered::AppEvent;

/// GTD 状态常量（与 NoteTask 保持一致; 内联解除 loro-crdt feature 依赖）。
const STATUS_INBOX: &str = "inbox";
const STATUS_DONE: &str = "done";
use crate::event_bus::projection::{Projection, ProjectionHealth};
use crate::traits::kv_store::KVStore;

/// 任务读模型行（TodayView 渲染所需字段全集）。
#[derive(Debug, Clone, PartialEq)]
pub struct TaskViewRow {
    pub task_id: String,
    /// 双锚定之来源笔记（DK-06: 任务保留来源笔记上下文）
    pub note_id: String,
    /// 双锚定之来源块（来源块的精确位置; 空串 = 无块锚定）
    pub block_id: String,
    pub title: String,
    /// GTD 状态: inbox/next/waiting/scheduled/someday/done/cancelled
    pub status: String,
    /// low/medium/high/urgent
    pub priority: String,
    /// Unix epoch 毫秒
    pub due_date: Option<i64>,
    /// 预估耗时（分钟; 0 = 未估）
    pub estimate_minutes: u32,
    /// 实际耗时累计（分钟 — 由番茄钟/计时会话累加, 禁止手改）
    pub actual_minutes: u32,
    /// 父任务 id（子任务归属; 空串 = 顶层任务）
    pub parent_task_id: String,
    /// 子任务进度（0.0-1.0, 由子任务完成比例**推导** — DK-06: 禁止手改;
    /// 无子任务的父任务返回自身完成状态: done=1.0 其余 0.0）
    pub progress: f32,
}

/// 全量数据源回调（全库任务行）。
pub type TaskSource = Box<dyn Fn() -> Vec<TaskViewRow> + Send + Sync>;

/// 任务投影。
pub struct TaskProjection {
    rows: RwLock<BTreeMap<String, TaskViewRow>>,
    kv: std::sync::Arc<dyn KVStore>,
    source: TaskSource,
}

const WATERMARK_KEY: &str = "projection.watermark.task";

impl TaskProjection {
    pub fn new(kv: std::sync::Arc<dyn KVStore>, source: TaskSource) -> Self {
        Self {
            rows: RwLock::new(BTreeMap::new()),
            kv,
            source,
        }
    }

    /// 按 GTD 状态查询（TodayView 四分区）。
    pub fn by_status(&self, status: &str) -> Vec<TaskViewRow> {
        self.rows
            .read()
            .unwrap()
            .values()
            .filter(|r| r.status == status)
            .cloned()
            .collect()
    }

    /// 今日视图: 未完成 + due <= 给定时刻（含逾期）。
    pub fn today(&self, now_ms: i64) -> Vec<TaskViewRow> {
        self.rows
            .read()
            .unwrap()
            .values()
            .filter(|r| r.status != STATUS_DONE && r.due_date.map(|d| d <= now_ms).unwrap_or(false))
            .cloned()
            .collect()
    }

    /// 播种/覆写单行（AI 行动项提取写入 — 幂等: 同 task_id 覆盖）。
    pub fn seed_row(
        &self,
        task_id: &str,
        note_id: &str,
        title: &str,
        status: &str,
        priority: &str,
        due_date: Option<i64>,
    ) {
        self.rows.write().unwrap().insert(
            task_id.to_string(),
            crate::l2_engines::task_projection::TaskViewRow {
                task_id: task_id.to_string(),
                note_id: note_id.to_string(),
                block_id: String::new(),
                parent_task_id: String::new(),
                progress: 0.0,
                title: title.to_string(),
                status: status.to_string(),
                priority: priority.to_string(),
                due_date,
                estimate_minutes: 0,
                actual_minutes: 0,
            },
        );
    }

    /// 时间追踪 — 实际耗时累加（DK-06: 由番茄钟/计时会话驱动,
    /// 禁止手改 progress/actual 语义的守卫点; actual 只能经此累加）。
    pub fn record_actual_minutes(&self, task_id: &str, minutes: u32) {
        let mut rows = self.rows.write().unwrap();
        if let Some(row) = rows.get_mut(task_id) {
            row.actual_minutes = row.actual_minutes.saturating_add(minutes);
        }
    }

    /// 预估耗时设置（estimate 可改 — 与 actual 分离）。
    pub fn set_estimate_minutes(&self, task_id: &str, minutes: u32) {
        let mut rows = self.rows.write().unwrap();
        if let Some(row) = rows.get_mut(task_id) {
            row.estimate_minutes = minutes;
        }
    }

    /// 时间偏差（TodayView 周回顾: 预计 vs 实际偏差率; 无预估返回 None）。
    pub fn deviation_ratio(&self, task_id: &str) -> Option<f64> {
        let rows = self.rows.read().unwrap();
        let row = rows.get(task_id)?;
        if row.estimate_minutes == 0 {
            return None;
        }
        Some(
            (row.actual_minutes as f64 - row.estimate_minutes as f64) / row.estimate_minutes as f64,
        )
    }

    /// 子任务进度推导（DK-06: progress 由子任务完成比例推导, 禁止手改）。
    ///
    /// 对每行重算 progress: 有子任务的父 = done 子任务数 / 子任务总数;
    /// 叶子任务 = 自身状态（done/cancelled → 1.0, 其余 0.0）。
    /// 返回发生变化的行数（供事件/测试断言）。
    pub fn derive_progress(&self) -> usize {
        let mut rows = self.rows.write().unwrap();
        // 收集 parent → children 映射
        let mut children: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        for r in rows.values() {
            if !r.parent_task_id.is_empty() {
                children
                    .entry(r.parent_task_id.clone())
                    .or_default()
                    .push(r.task_id.clone());
            }
        }
        let mut changed = 0usize;
        let ids: Vec<String> = rows.keys().cloned().collect();
        for id in &ids {
            let new_progress = match children.get(id) {
                Some(child_ids) => {
                    let total = child_ids.len();
                    let done = child_ids
                        .iter()
                        .filter(|c| rows.get(*c).map(|r| r.status == "done").unwrap_or(false))
                        .count();
                    done as f32 / total as f32
                }
                None => {
                    let st = rows.get(id).map(|r| r.status.as_str()).unwrap_or("");
                    if st == "done" || st == "cancelled" {
                        1.0
                    } else {
                        0.0
                    }
                }
            };
            if let Some(row) = rows.get_mut(id) {
                if (row.progress - new_progress).abs() > f32::EPSILON {
                    row.progress = new_progress;
                    changed += 1;
                }
            }
        }
        changed
    }

    /// 测试/工具访问器: 行表快照的可变句柄（progress 推导前置数据构造）。
    #[doc(hidden)]
    pub fn rows_for_test(
        &self,
    ) -> std::sync::RwLockWriteGuard<'_, std::collections::BTreeMap<String, TaskViewRow>> {
        self.rows.write().unwrap()
    }

    /// 统计（TodayView 头部）: (进行中, 已完成)。
    pub fn stats(&self) -> (usize, usize) {
        let rows = self.rows.read().unwrap();
        let done = STATUS_DONE;
        (
            rows.values().filter(|r| r.status != done).count(),
            rows.values().filter(|r| r.status == done).count(),
        )
    }

    fn note_created_seed(&self, note_id: &str, title: &str) {
        // NoteCreated 不带任务批次（当前 FFI 语义）→ 播种一条 inbox 任务
        // 占位行（V20 §3.2: 任务块一等公民的后续 PR 将携带结构化任务）
        let row = TaskViewRow {
            task_id: format!("seed:{note_id}"),
            note_id: note_id.to_string(),
            block_id: String::new(),
            parent_task_id: String::new(),
            progress: 0.0,
            title: title.to_string(),
            status: STATUS_INBOX.to_string(),
            priority: "medium".into(),
            due_date: None,
            estimate_minutes: 0,
            actual_minutes: 0,
        };
        self.rows.write().unwrap().insert(row.task_id.clone(), row);
    }

    fn note_deleted_cascade(&self, note_id: &str) {
        self.rows
            .write()
            .unwrap()
            .retain(|_, r| r.note_id != note_id);
    }

    fn row_count(&self) -> usize {
        self.rows.read().unwrap().len()
    }
}

#[async_trait]
impl Projection for TaskProjection {
    fn name(&self) -> &'static str {
        "task"
    }

    async fn watermark(&self) -> Result<u64, crate::Error> {
        match self.kv.get(WATERMARK_KEY).await? {
            Some(bytes) => String::from_utf8_lossy(&bytes)
                .trim()
                .parse::<u64>()
                .map_err(|e| crate::Error::Internal(format!("watermark parse: {e}"))),
            None => Ok(0),
        }
    }

    async fn apply(&self, event: &AppEvent) -> Result<(), crate::Error> {
        match event {
            AppEvent::NoteCreated { note_id, title, .. } => self.note_created_seed(note_id, title),
            AppEvent::NoteDeleted { note_id } => self.note_deleted_cascade(note_id),
            AppEvent::TaskStatusChanged {
                task_id,
                new_status,
                ..
            } => {
                // 幂等应用最终值（无该行时跳过 — 数据源行会在 rebuild 补）
                if let Some(row) = self.rows.write().unwrap().get_mut(task_id) {
                    row.status = new_status.clone();
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

    async fn set_watermark(&self, seq: u64) -> Result<(), crate::Error> {
        self.kv.set(WATERMARK_KEY, seq.to_string().as_bytes()).await
    }

    async fn verify(&self) -> Result<ProjectionHealth, crate::Error> {
        // 仅缺失判损坏（同 search 投影语义 — 投影行数 < 数据源行数）
        let expected = (self.source)().len();
        let actual = self.row_count();
        if actual < expected {
            return Ok(ProjectionHealth::Corrupted);
        }
        Ok(ProjectionHealth::Ok)
    }

    async fn rebuild(&self) -> Result<(), crate::Error> {
        info!("task projection: full rebuild");
        let all = (self.source)();
        self.rows.write().unwrap().clear();
        for row in all {
            self.rows.write().unwrap().insert(row.task_id.clone(), row);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_bus::layered::InMemoryEventQueue;
    use crate::event_bus::layered::LayeredEventBus;
    use crate::l1_infrastructure::storage_engine::MemoryKVStore;
    use std::sync::Arc;

    fn make(source_rows: Vec<TaskViewRow>) -> TaskProjection {
        let kv = Arc::new(MemoryKVStore::default());
        let src = Box::new(move || source_rows.clone());
        TaskProjection::new(kv, src)
    }

    fn row(id: &str, note: &str, status: &str, due: Option<i64>) -> TaskViewRow {
        TaskViewRow {
            task_id: id.into(),
            note_id: note.into(),
            block_id: String::new(),
            parent_task_id: String::new(),
            progress: 0.0,
            title: format!("task-{id}"),
            status: status.into(),
            priority: "medium".into(),
            due_date: due,
            estimate_minutes: 0,
            actual_minutes: 0,
        }
    }

    #[tokio::test]
    async fn note_created_seeds_and_deleted_cascades() {
        let bus = LayeredEventBus::new(Some(std::sync::Arc::new(
            crate::event_bus::layered::InMemoryEventQueue::new(),
        )));
        let p = make(Vec::new());
        bus.publish(AppEvent::NoteCreated {
            note_id: "n1".into(),
            title: "笔记A".into(),
            content: String::new(),
        });
        bus.catch_up(&p).await.unwrap();
        assert_eq!(p.by_status("inbox").len(), 1);
        assert_eq!(p.stats(), (1, 0));

        bus.publish(AppEvent::NoteDeleted {
            note_id: "n1".into(),
        });
        bus.catch_up(&p).await.unwrap();
        assert_eq!(p.row_count(), 0, "笔记删除级联清任务");
    }

    #[tokio::test]
    async fn status_change_applies_and_today_filters() {
        let bus = LayeredEventBus::new(Some(std::sync::Arc::new(
            crate::event_bus::layered::InMemoryEventQueue::new(),
        )));
        let p = make(vec![
            row("t1", "n1", "next", Some(1_000)),
            row("t2", "n1", "next", Some(9_999_999)),
        ]);
        // 事件侧建行: 经 NoteCreated 播种
        bus.publish(AppEvent::NoteCreated {
            note_id: "n1".into(),
            title: "T".into(),
            content: String::new(),
        });
        bus.catch_up(&p).await.unwrap();

        // 直接应用状态变更（幂等 — 不存在 t3 行则跳过）
        let p_ptr = &p;
        use crate::event_bus::projection::Projection as _;
        p_ptr
            .apply(&AppEvent::TaskStatusChanged {
                task_id: "t1".into(),
                old_status: "next".into(),
                new_status: "done".into(),
            })
            .await
            .unwrap();

        // verify: 源 2 行 vs 表 1 行（种子行）→ Corrupted → rebuild 覆盖
        // (rebuild 用源行 → t1(done)+t2(next))
        bus.catch_up(&p).await.unwrap();
        assert_eq!(p.by_status("done").len(), 1);
        // t1 done 且 due=1000 已完成不计; t2 未到期(due=9999999 > 5000)
        // → today(5000) 应为空; today(9999999) 才含 t2
        assert!(
            p.today(5_000).is_empty(),
            "均不满足今日条件: {:?}",
            p.today(5_000)
        );
        assert_eq!(
            p.today(9_999_999).len(),
            1,
            "t2 到期截止窗口内: {:?}",
            p.today(9_999_999)
        );
    }

    #[tokio::test]
    async fn rebuild_restores_from_source() {
        let p = make(vec![
            row("t1", "n1", "next", None),
            row("t2", "n2", "waiting", None),
        ]);
        let store = Arc::new(InMemoryEventQueue::new());
        let bus = LayeredEventBus::new(Some(store));
        bus.publish(AppEvent::NoteCreated {
            note_id: "n1".into(),
            title: "A".into(),
            content: String::new(),
        });
        // 表 1 行 < 源 2 行 → catch_up 尾部 verify Corrupted → rebuild
        bus.catch_up(&p).await.unwrap();
        assert_eq!(p.row_count(), 2);
        assert_eq!(p.by_status("waiting").len(), 1);
    }

    /// V26 DK-06: 双锚定字段 + 时间追踪语义（actual 只能累加; 偏差率）。
    #[tokio::test]
    async fn dual_anchor_and_time_tracking() {
        let bus = LayeredEventBus::new(Some(std::sync::Arc::new(
            crate::event_bus::layered::InMemoryEventQueue::new(),
        )));
        let p = make(Vec::new());
        bus.publish(AppEvent::NoteCreated {
            note_id: "n1".into(),
            title: "笔记A".into(),
            content: String::new(),
        });
        bus.catch_up(&p).await.unwrap();

        let row = p.by_status("inbox").into_iter().next().expect("seeded");
        assert_eq!(row.note_id, "n1");
        assert_eq!(row.block_id, "", "无块锚定为空");
        assert_eq!(row.estimate_minutes, 0);
        assert_eq!(row.actual_minutes, 0);

        p.set_estimate_minutes(&row.task_id, 25);
        p.record_actual_minutes(&row.task_id, 30);
        p.record_actual_minutes(&row.task_id, 15);
        let after = p.by_status("inbox").into_iter().next().unwrap();
        assert_eq!(after.estimate_minutes, 25);
        assert_eq!(after.actual_minutes, 45, "两次累加 30+15");

        let dev = p.deviation_ratio(&row.task_id).unwrap();
        assert!((dev - 0.8).abs() < 1e-9, "deviation {dev}");
        assert!(p.deviation_ratio("__missing__").is_none());
    }

    /// V26 DK-06: 子任务进度推导 — 禁止手改, 由完成比例计算。
    #[tokio::test]
    async fn parent_progress_derived_from_children() {
        let p = make(Vec::new());
        // 构造父子: parent(P) + 子 c1(done) c2(next) — 手工 upsert 不可用
        // （upsert 无 parent 字段）→ 直接经 derive 前先塞行:
        // 借 make() source 提供行会绕过 rows map——derive 基于 rows,
        // 所以先播种再改 status 模拟:
        let bus = LayeredEventBus::new(Some(std::sync::Arc::new(
            crate::event_bus::layered::InMemoryEventQueue::new(),
        )));
        for (nid, _t) in [("p1", "父"), ("c1", "子一"), ("c2", "子二")] {
            bus.publish(AppEvent::NoteCreated {
                note_id: nid.into(),
                title: _t.into(),
                content: String::new(),
            });
        }
        bus.catch_up(&p).await.unwrap();
        // 播种行 task_id = seed:{note_id}; 手动设定父子关系与状态
        {
            let mut rows = p.rows_for_test();
            // parent: p1, 子: c1/c2; c1 done
            if let Some(r) = rows.get_mut("seed:p1") {
                r.status = "next".into();
            }
            for cid in ["seed:c1", "seed:c2"] {
                if let Some(r) = rows.get_mut(cid) {
                    r.parent_task_id = "seed:p1".into();
                }
            }
            if let Some(r) = rows.get_mut("seed:c1") {
                r.status = "done".into();
            }
        }
        let changed = p.derive_progress();
        assert!(
            changed >= 2,
            "有变更行（父 0→0.5, c1 0→1.0; c2 恒 0.0 无变更）, got {changed}"
        );
        let parent = p
            .by_status("next")
            .into_iter()
            .find(|r| r.task_id == "seed:p1")
            .expect("parent");
        assert!(
            (parent.progress - 0.5).abs() < 1e-6,
            "1/2 子任务完成 → 0.5, got {}",
            parent.progress
        );
    }
}
