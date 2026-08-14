//! TodayView 今日视图
//!
//! 实现聚合视图、时间线视图、专注模式（番茄钟）、每日回顾。
//!
//! # 简化说明
//! - 聚合视图的 Zustand 缓存与 EventBus 增量更新用内存 `Arc<RwLock<>>` 模拟。
//! - 番茄钟使用 `tokio::time` 进行倒计时；测试中使用短周期以避免阻塞。
//! - 虚拟列表（react-window）不真实渲染，仅返回切片数据 + 总数。

use chrono::{DateTime, Duration, NaiveDate, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info};
use uuid::Uuid;

// ============================================================================
// SubTask 3.7.1: 聚合视图架构
// ============================================================================

/// 今日待办项（来自 GTD 系统）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodayTodo {
    pub id: String,
    pub title: String,
    pub due_at: Option<DateTime<Utc>>,
    pub priority: u8,
    pub completed: bool,
    pub estimated_minutes: Option<u32>,
}

/// 今日日程项（来自日历）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodayEvent {
    pub id: String,
    pub title: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub location: Option<String>,
}

/// 今日习惯打卡
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodayHabit {
    pub id: String,
    pub name: String,
    pub completed: bool,
    /// 连续天数
    pub streak: u32,
}

/// 今日视图数据
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TodayViewData {
    pub date: NaiveDate,
    pub todos: Vec<TodayTodo>,
    pub events: Vec<TodayEvent>,
    pub habits: Vec<TodayHabit>,
    pub focus_stats: Option<FocusStats>,
    pub generated_at: DateTime<Utc>,
}

impl TodayViewData {
    pub fn for_date(date: NaiveDate) -> Self {
        Self {
            date,
            todos: Vec::new(),
            events: Vec::new(),
            habits: Vec::new(),
            focus_stats: None,
            generated_at: Utc::now(),
        }
    }

    /// 待办完成数
    pub fn completed_todo_count(&self) -> usize {
        self.todos.iter().filter(|t| t.completed).count()
    }

    /// 待办完成率（0.0 ~ 1.0）
    pub fn completion_rate(&self) -> f32 {
        if self.todos.is_empty() {
            return 0.0;
        }
        self.completed_todo_count() as f32 / self.todos.len() as f32
    }

    /// 习惯连续天数总和
    pub fn habit_streak_total(&self) -> u32 {
        self.habits.iter().map(|h| h.streak).sum()
    }
}

/// 聚合器：从多个来源（todo / event / habit / focus）聚合今日视图数据
///
/// 内部维护缓存（模拟 Zustand store），支持增量更新。
pub struct TodayViewAggregator {
    cache: Arc<RwLock<HashMap<NaiveDate, TodayViewData>>>,
    /// EventBus 订阅者（mock：只是一组回调 ID）
    subscribers: Arc<RwLock<Vec<String>>>,
}

impl Default for TodayViewAggregator {
    fn default() -> Self {
        Self::new()
    }
}

impl TodayViewAggregator {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            subscribers: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// 订阅增量更新（mock：仅记录订阅者）
    pub fn subscribe(&self, subscriber_id: impl Into<String>) {
        self.subscribers.write().push(subscriber_id.into());
    }

    pub fn subscriber_count(&self) -> usize {
        self.subscribers.read().len()
    }

    /// 聚合多个来源数据，写入缓存
    pub fn aggregate(
        &self,
        date: NaiveDate,
        todos: Vec<TodayTodo>,
        events: Vec<TodayEvent>,
        habits: Vec<TodayHabit>,
        focus_stats: Option<FocusStats>,
    ) -> TodayViewData {
        let data = TodayViewData {
            date,
            todos,
            events,
            habits,
            focus_stats,
            generated_at: Utc::now(),
        };
        self.cache.write().insert(date, data.clone());
        info!(date = %date, subs = self.subscribers.read().len(), "today view aggregated");
        data
    }

    /// 增量更新：替换某天的待办列表（模拟 EventBus 推送触发）
    pub fn update_todos(&self, date: NaiveDate, todos: Vec<TodayTodo>) -> Option<TodayViewData> {
        let mut cache = self.cache.write();
        let data = cache.get_mut(&date)?;
        data.todos = todos;
        data.generated_at = Utc::now();
        Some(data.clone())
    }

    /// 从缓存读取
    pub fn get(&self, date: NaiveDate) -> Option<TodayViewData> {
        self.cache.read().get(&date).cloned()
    }
}

// ============================================================================
// SubTask 3.7.2: 时间线视图
// ============================================================================

/// 时间线粒度
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum TimelineGranularity {
    Day,
    Week,
    Month,
}

/// 时间线条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineItem {
    pub id: String,
    pub title: String,
    pub start: DateTime<Utc>,
    pub end: Option<DateTime<Utc>>,
    pub kind: TimelineItemKind,
    pub source_id: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum TimelineItemKind {
    Event,
    Todo,
    FocusSession,
    Habit,
}

/// 时间线视图（支持虚拟列表切片）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineView {
    pub granularity: TimelineGranularity,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub items: Vec<TimelineItem>,
    pub total: usize,
}

impl TimelineView {
    /// 构建时间线
    pub fn build(
        granularity: TimelineGranularity,
        start: DateTime<Utc>,
        mut items: Vec<TimelineItem>,
    ) -> Self {
        items.sort_by_key(|i| i.start);
        let total = items.len();
        let end = match granularity {
            TimelineGranularity::Day => start + Duration::days(1),
            TimelineGranularity::Week => start + Duration::weeks(1),
            TimelineGranularity::Month => start + Duration::days(30),
        };
        Self {
            granularity,
            start,
            end,
            items,
            total,
        }
    }

    /// 虚拟列表切片（模拟 react-window）
    pub fn slice(&self, offset: usize, limit: usize) -> &[TimelineItem] {
        if offset >= self.items.len() {
            return &[];
        }
        let end = (offset + limit).min(self.items.len());
        &self.items[offset..end]
    }

    /// 拖拽改写某待办的 due_date
    pub fn rewrite_due_date(&mut self, item_id: &str, new_start: DateTime<Utc>) -> bool {
        if let Some(item) = self.items.iter_mut().find(|i| i.id == item_id) {
            item.start = new_start;
            return true;
        }
        false
    }
}

// ============================================================================
// SubTask 3.7.3: 专注模式
// ============================================================================

/// 专注会话
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FocusSession {
    pub id: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub planned_duration_minutes: u32,
    pub actual_duration_minutes: Option<u32>,
    pub completed: bool,
    pub task_id: Option<String>,
    pub white_noise: WhiteNoiseKind,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum WhiteNoiseKind {
    None,
    Rain,
    Forest,
    BrownNoise,
    Ocean,
}

/// 专注统计
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FocusStats {
    pub total_sessions: u32,
    pub completed_sessions: u32,
    pub total_focus_minutes: u32,
    pub longest_streak: u32,
}

impl FocusStats {
    pub fn record(&mut self, session: &FocusSession) {
        self.total_sessions += 1;
        if session.completed {
            self.completed_sessions += 1;
            self.total_focus_minutes += session
                .actual_duration_minutes
                .unwrap_or(session.planned_duration_minutes);
        }
    }

    pub fn completion_rate(&self) -> f32 {
        if self.total_sessions == 0 {
            return 0.0;
        }
        self.completed_sessions as f32 / self.total_sessions as f32
    }
}

/// 番茄钟（默认 25min 工作 / 5min 休息）
pub struct PomodoroTimer {
    pub work_minutes: u32,
    pub break_minutes: u32,
    state: Arc<RwLock<PomodoroState>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PomodoroState {
    pub phase: PomodoroPhase,
    pub remaining_seconds: u32,
    pub completed_work_cycles: u32,
    pub running: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum PomodoroPhase {
    Work,
    Break,
}

impl Default for PomodoroTimer {
    fn default() -> Self {
        Self::new(25, 5)
    }
}

impl PomodoroTimer {
    pub fn new(work_minutes: u32, break_minutes: u32) -> Self {
        Self {
            work_minutes,
            break_minutes,
            state: Arc::new(RwLock::new(PomodoroState {
                phase: PomodoroPhase::Work,
                remaining_seconds: work_minutes * 60,
                completed_work_cycles: 0,
                running: false,
            })),
        }
    }

    pub fn state(&self) -> PomodoroState {
        self.state.read().clone()
    }

    pub fn start(&self) {
        self.state.write().running = true;
        debug!("pomodoro started");
    }

    pub fn pause(&self) {
        self.state.write().running = false;
    }

    pub fn reset(&self) {
        let mut s = self.state.write();
        s.phase = PomodoroPhase::Work;
        s.remaining_seconds = self.work_minutes * 60;
        s.completed_work_cycles = 0;
        s.running = false;
    }

    /// 推进 1 秒（同步模拟，便于测试）
    pub fn tick(&self) -> PomodoroPhase {
        let mut s = self.state.write();
        if !s.running {
            return s.phase;
        }
        if s.remaining_seconds > 0 {
            s.remaining_seconds -= 1;
        }
        if s.remaining_seconds == 0 {
            // 切换阶段
            match s.phase {
                PomodoroPhase::Work => {
                    s.completed_work_cycles += 1;
                    s.phase = PomodoroPhase::Break;
                    s.remaining_seconds = self.break_minutes * 60;
                }
                PomodoroPhase::Break => {
                    s.phase = PomodoroPhase::Work;
                    s.remaining_seconds = self.work_minutes * 60;
                }
            }
        }
        s.phase
    }

    /// 真实异步倒计时（tokio）。运行到下一个阶段切换为止。
    pub async fn run_until_phase_change(&self) -> PomodoroPhase {
        let start_phase = self.state.read().phase;
        loop {
            self.tick();
            if self.state.read().phase != start_phase {
                return self.state.read().phase;
            }
            // 短 sleep 避免忙等
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
    }
}

/// 专注模式
pub struct FocusMode {
    timer: PomodoroTimer,
    sessions: Arc<RwLock<Vec<FocusSession>>>,
    stats: Arc<RwLock<FocusStats>>,
}

impl FocusMode {
    pub fn new(work_minutes: u32, break_minutes: u32) -> Self {
        Self {
            timer: PomodoroTimer::new(work_minutes, break_minutes),
            sessions: Arc::new(RwLock::new(Vec::new())),
            stats: Arc::new(RwLock::new(FocusStats::default())),
        }
    }

    /// 开始一次专注会话
    pub fn start_session(
        &self,
        task_id: Option<String>,
        white_noise: WhiteNoiseKind,
    ) -> FocusSession {
        self.timer.start();
        let session = FocusSession {
            id: Uuid::new_v4().to_string(),
            started_at: Utc::now(),
            ended_at: None,
            planned_duration_minutes: self.timer.work_minutes,
            actual_duration_minutes: None,
            completed: false,
            task_id,
            white_noise,
        };
        debug!(session_id = %session.id, "focus session started");
        session
    }

    /// 结束会话并记录
    pub fn end_session(&self, mut session: FocusSession, completed: bool) -> FocusSession {
        session.ended_at = Some(Utc::now());
        session.actual_duration_minutes = Some(session.planned_duration_minutes);
        session.completed = completed;
        self.stats.write().record(&session);
        self.sessions.write().push(session.clone());
        info!(session_id = %session.id, completed, "focus session ended");
        session
    }

    pub fn timer(&self) -> &PomodoroTimer {
        &self.timer
    }

    pub fn stats(&self) -> FocusStats {
        self.stats.read().clone()
    }

    pub fn sessions(&self) -> Vec<FocusSession> {
        self.sessions.read().clone()
    }
}

// ============================================================================
// SubTask 3.7.4: 每日回顾
// ============================================================================

/// 每日报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyReport {
    pub date: NaiveDate,
    pub task_completion_rate: f32,
    pub completed_tasks: usize,
    pub total_tasks: usize,
    pub time_allocation_minutes: u32,
    pub habit_continuity: u32,
    pub focus_sessions: u32,
    pub highlights: Vec<String>,
}

/// 回顾历史
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReviewHistory {
    pub reports: Vec<DailyReport>,
}

impl ReviewHistory {
    pub fn add(&mut self, report: DailyReport) {
        self.reports.push(report);
    }

    pub fn for_date(&self, date: NaiveDate) -> Option<&DailyReport> {
        self.reports.iter().find(|r| r.date == date)
    }

    pub fn date_range(&self) -> Option<(NaiveDate, NaiveDate)> {
        if self.reports.is_empty() {
            return None;
        }
        let mut dates: Vec<NaiveDate> = self.reports.iter().map(|r| r.date).collect();
        dates.sort();
        let first = *dates.first().expect("dates non-empty (checked above)");
        let last = *dates.last().expect("dates non-empty (checked above)");
        Some((first, last))
    }
}

/// 每日回顾生成器
pub struct DailyReview;

impl DailyReview {
    /// 基于今日视图数据生成报告
    pub fn generate(data: &TodayViewData) -> DailyReport {
        let completed = data.completed_todo_count();
        let total = data.todos.len();
        let time_allocation: u32 = data.todos.iter().filter_map(|t| t.estimated_minutes).sum();
        let habit_continuity = data.habit_streak_total();
        let focus_sessions = data
            .focus_stats
            .as_ref()
            .map(|s| s.total_sessions)
            .unwrap_or(0);

        let mut highlights = Vec::new();
        if data.completion_rate() >= 0.8 {
            highlights.push("🎉 完成率达到 80% 以上".to_string());
        }
        if habit_continuity >= 7 {
            highlights.push("🔥 习惯连续 7 天以上".to_string());
        }
        if focus_sessions >= 4 {
            highlights.push("🍅 完成 4 个以上番茄钟".to_string());
        }
        if highlights.is_empty() {
            highlights.push("今日继续加油".to_string());
        }

        DailyReport {
            date: data.date,
            task_completion_rate: data.completion_rate(),
            completed_tasks: completed,
            total_tasks: total,
            time_allocation_minutes: time_allocation,
            habit_continuity,
            focus_sessions,
            highlights,
        }
    }
}

// ============================================================================
// 顶层 TodayView 聚合
// ============================================================================

/// 今日视图顶层
pub struct TodayView {
    pub aggregator: TodayViewAggregator,
    pub focus: FocusMode,
    pub review_history: Arc<RwLock<ReviewHistory>>,
}

impl Default for TodayView {
    fn default() -> Self {
        Self::new()
    }
}

impl TodayView {
    pub fn new() -> Self {
        Self {
            aggregator: TodayViewAggregator::new(),
            focus: FocusMode::new(25, 5),
            review_history: Arc::new(RwLock::new(ReviewHistory::default())),
        }
    }

    /// 生成并保存今日报告
    pub fn generate_daily_report(&self, date: NaiveDate) -> Option<DailyReport> {
        let data = self.aggregator.get(date)?;
        let report = DailyReview::generate(&data);
        self.review_history.write().add(report.clone());
        Some(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2024, 6, 1).unwrap()
    }

    fn make_todo(id: &str, completed: bool, mins: Option<u32>) -> TodayTodo {
        TodayTodo {
            id: id.to_string(),
            title: format!("task-{}", id),
            due_at: None,
            priority: 1,
            completed,
            estimated_minutes: mins,
        }
    }

    // --- Aggregator ---

    #[test]
    fn test_aggregator_aggregate_and_get() {
        let agg = TodayViewAggregator::new();
        agg.subscribe("sub1");
        let data = agg.aggregate(
            today(),
            vec![make_todo("t1", false, Some(30))],
            vec![],
            vec![],
            None,
        );
        assert_eq!(data.todos.len(), 1);
        assert_eq!(agg.subscriber_count(), 1);
        let cached = agg.get(today()).unwrap();
        assert_eq!(cached.todos.len(), 1);
    }

    #[test]
    fn test_aggregator_incremental_update() {
        let agg = TodayViewAggregator::new();
        agg.aggregate(
            today(),
            vec![make_todo("t1", false, None)],
            vec![],
            vec![],
            None,
        );
        let updated = agg
            .update_todos(today(), vec![make_todo("t1", true, None)])
            .unwrap();
        assert!(updated.todos[0].completed);
        // 生成时间应刷新
        let cached = agg.get(today()).unwrap();
        assert!(cached.todos[0].completed);
    }

    #[test]
    fn test_today_view_data_completion_rate() {
        let mut data = TodayViewData::for_date(today());
        data.todos = vec![
            make_todo("t1", true, None),
            make_todo("t2", true, None),
            make_todo("t3", false, None),
            make_todo("t4", false, None),
        ];
        assert_eq!(data.completed_todo_count(), 2);
        assert_eq!(data.completion_rate(), 0.5);
    }

    #[test]
    fn test_today_view_data_empty_rate() {
        let data = TodayViewData::for_date(today());
        assert_eq!(data.completion_rate(), 0.0);
    }

    #[test]
    fn test_today_view_habit_streak() {
        let mut data = TodayViewData::for_date(today());
        data.habits = vec![
            TodayHabit {
                id: "h1".into(),
                name: "read".into(),
                completed: true,
                streak: 5,
            },
            TodayHabit {
                id: "h2".into(),
                name: "run".into(),
                completed: false,
                streak: 3,
            },
        ];
        assert_eq!(data.habit_streak_total(), 8);
    }

    // --- Timeline ---

    fn make_item(id: &str, hour: i64, kind: TimelineItemKind) -> TimelineItem {
        let start =
            Utc.from_utc_datetime(&today().and_hms_opt(0, 0, 0).unwrap()) + Duration::hours(hour);
        TimelineItem {
            id: id.to_string(),
            title: format!("item-{}", id),
            start,
            end: None,
            kind,
            source_id: id.to_string(),
        }
    }

    #[test]
    fn test_timeline_build_sorts() {
        let items = vec![
            make_item("a", 10, TimelineItemKind::Event),
            make_item("b", 2, TimelineItemKind::Todo),
            make_item("c", 5, TimelineItemKind::Event),
        ];
        let tv = TimelineView::build(
            TimelineGranularity::Day,
            Utc.from_utc_datetime(&today().and_hms_opt(0, 0, 0).unwrap()),
            items,
        );
        assert_eq!(tv.items.len(), 3);
        assert_eq!(tv.items[0].id, "b");
        assert_eq!(tv.items[1].id, "c");
        assert_eq!(tv.items[2].id, "a");
        assert_eq!(tv.total, 3);
    }

    #[test]
    fn test_timeline_slice_virtual_list() {
        let items: Vec<_> = (0..10)
            .map(|i| make_item(&format!("i{}", i), i, TimelineItemKind::Event))
            .collect();
        let start = Utc.from_utc_datetime(&today().and_hms_opt(0, 0, 0).unwrap());
        let tv = TimelineView::build(TimelineGranularity::Day, start, items);
        let page = tv.slice(2, 3);
        assert_eq!(page.len(), 3);
        assert_eq!(page[0].id, "i2");
        assert_eq!(page[2].id, "i4");
        // 越界返回空
        assert!(tv.slice(100, 5).is_empty());
        // 末尾不足
        let tail = tv.slice(8, 10);
        assert_eq!(tail.len(), 2);
    }

    #[test]
    fn test_timeline_granularity_end() {
        let start = Utc.from_utc_datetime(&today().and_hms_opt(0, 0, 0).unwrap());
        let day = TimelineView::build(TimelineGranularity::Day, start, vec![]);
        assert_eq!(day.end - day.start, Duration::days(1));
        let week = TimelineView::build(TimelineGranularity::Week, start, vec![]);
        assert_eq!(week.end - week.start, Duration::weeks(1));
        let month = TimelineView::build(TimelineGranularity::Month, start, vec![]);
        assert_eq!(month.end - month.start, Duration::days(30));
    }

    #[test]
    fn test_timeline_rewrite_due_date() {
        let items = vec![make_item("a", 10, TimelineItemKind::Todo)];
        let start = Utc.from_utc_datetime(&today().and_hms_opt(0, 0, 0).unwrap());
        let mut tv = TimelineView::build(TimelineGranularity::Day, start, items);
        let new_start = Utc.from_utc_datetime(&today().and_hms_opt(14, 0, 0).unwrap());
        assert!(tv.rewrite_due_date("a", new_start));
        assert_eq!(tv.items[0].start, new_start);
        assert!(!tv.rewrite_due_date("nonexistent", new_start));
    }

    // --- Pomodoro / Focus ---

    #[test]
    fn test_pomodoro_default_cycle() {
        let timer = PomodoroTimer::new(25, 5);
        let s = timer.state();
        assert_eq!(s.phase, PomodoroPhase::Work);
        assert_eq!(s.remaining_seconds, 25 * 60);
        assert!(!s.running);
    }

    #[test]
    fn test_pomodoro_tick_does_not_advance_when_paused() {
        let timer = PomodoroTimer::new(1, 1);
        // 未 start：tick 不应该推进
        timer.tick();
        assert_eq!(timer.state().remaining_seconds, 60);
    }

    #[test]
    fn test_pomodoro_phase_switch() {
        let timer = PomodoroTimer::new(1, 1); // 1 min work / 1 min break
        timer.start();
        let work_secs = 60;
        for _ in 0..work_secs {
            timer.tick();
        }
        // 60 次 tick 后应切换到 Break
        let s = timer.state();
        assert_eq!(s.phase, PomodoroPhase::Break);
        assert_eq!(s.remaining_seconds, 60);
        assert_eq!(s.completed_work_cycles, 1);
    }

    #[test]
    fn test_pomodoro_break_to_work() {
        let timer = PomodoroTimer::new(1, 1);
        timer.start();
        // Work → Break
        for _ in 0..60 {
            timer.tick();
        }
        assert_eq!(timer.state().phase, PomodoroPhase::Break);
        // Break → Work
        for _ in 0..60 {
            timer.tick();
        }
        assert_eq!(timer.state().phase, PomodoroPhase::Work);
        assert_eq!(timer.state().completed_work_cycles, 1);
    }

    #[test]
    fn test_pomodoro_reset() {
        let timer = PomodoroTimer::new(1, 1);
        timer.start();
        for _ in 0..30 {
            timer.tick();
        }
        timer.reset();
        let s = timer.state();
        assert_eq!(s.phase, PomodoroPhase::Work);
        assert_eq!(s.remaining_seconds, 60);
        assert!(!s.running);
        assert_eq!(s.completed_work_cycles, 0);
    }

    #[test]
    fn test_focus_session_recorded() {
        let fm = FocusMode::new(25, 5);
        let session = fm.start_session(Some("task1".into()), WhiteNoiseKind::Rain);
        let ended = fm.end_session(session, true);
        assert!(ended.completed);
        assert!(ended.ended_at.is_some());
        let stats = fm.stats();
        assert_eq!(stats.total_sessions, 1);
        assert_eq!(stats.completed_sessions, 1);
        assert_eq!(stats.total_focus_minutes, 25);
    }

    #[test]
    fn test_focus_stats_incomplete_not_counted() {
        let mut stats = FocusStats::default();
        let session = FocusSession {
            id: "s1".into(),
            started_at: Utc::now(),
            ended_at: Some(Utc::now()),
            planned_duration_minutes: 25,
            actual_duration_minutes: Some(10),
            completed: false,
            task_id: None,
            white_noise: WhiteNoiseKind::None,
        };
        stats.record(&session);
        assert_eq!(stats.total_sessions, 1);
        assert_eq!(stats.completed_sessions, 0);
        assert_eq!(stats.total_focus_minutes, 0);
    }

    #[test]
    fn test_focus_stats_completion_rate() {
        let mut stats = FocusStats::default();
        for completed in [true, false, true] {
            let s = FocusSession {
                id: "x".into(),
                started_at: Utc::now(),
                ended_at: Some(Utc::now()),
                planned_duration_minutes: 25,
                actual_duration_minutes: Some(25),
                completed,
                task_id: None,
                white_noise: WhiteNoiseKind::None,
            };
            stats.record(&s);
        }
        assert_eq!(stats.completion_rate(), 2.0 / 3.0);
    }

    #[tokio::test]
    async fn test_pomodoro_async_phase_change() {
        let timer = PomodoroTimer::new(1, 1);
        timer.start();
        // 设置一个很小的剩余时间
        {
            let mut s = timer.state.write();
            s.remaining_seconds = 2;
        }
        let phase = timer.run_until_phase_change().await;
        assert_eq!(phase, PomodoroPhase::Break);
    }

    // --- Daily Review ---

    #[test]
    fn test_daily_report_generation() {
        let mut data = TodayViewData::for_date(today());
        data.todos = vec![
            make_todo("t1", true, Some(25)),
            make_todo("t2", true, Some(25)),
            make_todo("t3", false, Some(50)),
            make_todo("t4", false, Some(50)),
        ];
        data.habits = vec![TodayHabit {
            id: "h1".into(),
            name: "read".into(),
            completed: true,
            streak: 10,
        }];
        data.focus_stats = Some(FocusStats {
            total_sessions: 5,
            completed_sessions: 5,
            total_focus_minutes: 125,
            longest_streak: 0,
        });
        let report = DailyReview::generate(&data);
        assert_eq!(report.date, today());
        assert_eq!(report.completed_tasks, 2);
        assert_eq!(report.total_tasks, 4);
        assert_eq!(report.time_allocation_minutes, 150);
        assert_eq!(report.habit_continuity, 10);
        assert_eq!(report.focus_sessions, 5);
        assert!(!report.highlights.is_empty());
    }

    #[test]
    fn test_daily_report_highlights_triggered() {
        let mut data = TodayViewData::for_date(today());
        data.todos = vec![
            make_todo("t1", true, None),
            make_todo("t2", true, None),
            make_todo("t3", true, None),
            make_todo("t4", true, None),
            make_todo("t5", true, None),
        ]; // 5/5 = 1.0
        data.habits = vec![TodayHabit {
            id: "h1".into(),
            name: "run".into(),
            completed: true,
            streak: 14,
        }];
        data.focus_stats = Some(FocusStats {
            total_sessions: 6,
            completed_sessions: 6,
            total_focus_minutes: 150,
            longest_streak: 0,
        });
        let report = DailyReview::generate(&data);
        // 三个 highlight 条件全部触发
        assert!(report.highlights.iter().any(|h| h.contains("80%")));
        assert!(report.highlights.iter().any(|h| h.contains("7")));
        assert!(report.highlights.iter().any(|h| h.contains("4")));
    }

    #[test]
    fn test_daily_report_empty_highlights_default() {
        let data = TodayViewData::for_date(today());
        let report = DailyReview::generate(&data);
        assert_eq!(report.highlights.len(), 1);
        assert_eq!(report.task_completion_rate, 0.0);
    }

    #[test]
    fn test_review_history() {
        let mut history = ReviewHistory::default();
        let date1 = NaiveDate::from_ymd_opt(2024, 6, 1).unwrap();
        let date2 = NaiveDate::from_ymd_opt(2024, 6, 2).unwrap();
        history.add(DailyReport {
            date: date1,
            task_completion_rate: 0.5,
            completed_tasks: 1,
            total_tasks: 2,
            time_allocation_minutes: 30,
            habit_continuity: 1,
            focus_sessions: 0,
            highlights: vec![],
        });
        history.add(DailyReport {
            date: date2,
            task_completion_rate: 1.0,
            completed_tasks: 2,
            total_tasks: 2,
            time_allocation_minutes: 60,
            habit_continuity: 2,
            focus_sessions: 1,
            highlights: vec![],
        });
        assert_eq!(history.reports.len(), 2);
        assert!(history.for_date(date1).is_some());
        let (start, end) = history.date_range().unwrap();
        assert_eq!(start, date1);
        assert_eq!(end, date2);
    }

    #[test]
    fn test_today_view_top_level_generate_report() {
        let tv = TodayView::new();
        tv.aggregator.aggregate(
            today(),
            vec![make_todo("t1", true, None)],
            vec![],
            vec![],
            None,
        );
        let report = tv.generate_daily_report(today()).unwrap();
        assert_eq!(report.date, today());
        assert_eq!(report.completed_tasks, 1);
        // 第二次生成：history 应有 2 条
        let _ = tv.generate_daily_report(today()).unwrap();
        assert_eq!(tv.review_history.read().reports.len(), 2);
    }
}
