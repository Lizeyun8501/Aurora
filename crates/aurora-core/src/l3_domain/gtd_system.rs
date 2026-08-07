//! GTD 效能系统（GTD System 2.0）
//!
//! 实现任务状态机、项目层级嵌套、收件箱、重复任务、提醒、习惯追踪、自动化规则。

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use parking_lot::RwLock;
use tracing::warn;
use uuid::Uuid;

/// 任务唯一标识
pub type TaskId = String;
/// 项目唯一标识
pub type ProjectId = String;

/// 任务状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    /// 收件箱
    Inbox,
    /// 已澄清
    Clarified,
    /// 已组织
    Organized,
    /// 已排期
    Scheduled,
    /// 进行中
    Doing,
    /// 已完成
    Done,
    /// 已归档
    Archived,
}

impl TaskStatus {
    /// 获取可流转的下一状态
    pub fn next_states(&self) -> Vec<TaskStatus> {
        match self {
            TaskStatus::Inbox => vec![TaskStatus::Clarified, TaskStatus::Archived],
            TaskStatus::Clarified => vec![TaskStatus::Organized, TaskStatus::Archived],
            TaskStatus::Organized => vec![TaskStatus::Scheduled, TaskStatus::Doing, TaskStatus::Archived],
            TaskStatus::Scheduled => vec![TaskStatus::Doing, TaskStatus::Organized],
            TaskStatus::Doing => vec![TaskStatus::Done, TaskStatus::Organized],
            TaskStatus::Done => vec![TaskStatus::Archived, TaskStatus::Doing],
            TaskStatus::Archived => vec![TaskStatus::Inbox, TaskStatus::Clarified],
        }
    }

    pub fn can_transition_to(&self, next: &TaskStatus) -> bool {
        self.next_states().contains(next)
    }
}

/// 优先级
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    Low,
    Medium,
    High,
    Urgent,
}

/// 任务结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    pub title: String,
    pub description: Option<String>,
    pub status: TaskStatus,
    pub priority: Priority,
    pub project_id: Option<ProjectId>,
    pub parent_id: Option<TaskId>,
    pub due_date: Option<chrono::DateTime<chrono::Utc>>,
    pub scheduled_date: Option<chrono::DateTime<chrono::Utc>>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub tags: Vec<String>,
    pub context: Option<String>,
    pub estimated_minutes: Option<u32>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub energy_level: Option<EnergyLevel>,
}

/// 精力水平
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EnergyLevel {
    Low,
    Medium,
    High,
}

impl Task {
    pub fn new(title: impl Into<String>) -> Self {
        let now = chrono::Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            title: title.into(),
            description: None,
            status: TaskStatus::Inbox,
            priority: Priority::Medium,
            project_id: None,
            parent_id: None,
            due_date: None,
            scheduled_date: None,
            completed_at: None,
            tags: Vec::new(),
            context: None,
            estimated_minutes: None,
            created_at: now,
            updated_at: now,
            energy_level: None,
        }
    }

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    pub fn with_priority(mut self, priority: Priority) -> Self {
        self.priority = priority;
        self
    }

    pub fn with_due_date(mut self, due: chrono::DateTime<chrono::Utc>) -> Self {
        self.due_date = Some(due);
        self
    }

    pub fn with_project(mut self, project_id: impl Into<String>) -> Self {
        self.project_id = Some(project_id.into());
        self
    }

    pub fn with_parent(mut self, parent_id: impl Into<String>) -> Self {
        self.parent_id = Some(parent_id.into());
        self
    }

    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = Some(context.into());
        self
    }

    pub fn with_energy(mut self, energy: EnergyLevel) -> Self {
        self.energy_level = Some(energy);
        self
    }

    pub fn transition_to(&mut self, new_status: TaskStatus) -> Result<(), String> {
        if !self.status.can_transition_to(&new_status) {
            return Err(format!(
                "Cannot transition from {:?} to {:?}",
                self.status, new_status
            ));
        }
        self.status = new_status.clone();
        if new_status == TaskStatus::Done {
            self.completed_at = Some(chrono::Utc::now());
        }
        self.updated_at = chrono::Utc::now();
        Ok(())
    }

    pub fn is_overdue(&self) -> bool {
        match self.due_date {
            Some(due) if self.status != TaskStatus::Done && self.status != TaskStatus::Archived => {
                chrono::Utc::now() > due
            }
            _ => false,
        }
    }
}

/// 项目结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: ProjectId,
    pub title: String,
    pub description: Option<String>,
    pub goal: Option<String>,
    pub status: ProjectStatus,
    pub parent_id: Option<ProjectId>,
    pub tags: Vec<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// 项目状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectStatus {
    Active,
    OnHold,
    Completed,
    Cancelled,
}

impl Project {
    pub fn new(title: impl Into<String>) -> Self {
        let now = chrono::Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            title: title.into(),
            description: None,
            goal: None,
            status: ProjectStatus::Active,
            parent_id: None,
            tags: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }
}

/// Closure Table 层级关系条目
#[derive(Debug, Clone)]
pub struct ClosureEntry {
    pub ancestor_id: String,
    pub descendant_id: String,
    pub depth: u32,
}

/// 重复规则（简化版 RRULE）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecurrenceRule {
    pub frequency: RecurrenceFrequency,
    pub interval: u32,
    pub count: Option<u32>,
    pub until: Option<chrono::DateTime<chrono::Utc>>,
    pub by_weekday: Option<Vec<chrono::Weekday>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecurrenceFrequency {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

impl RecurrenceRule {
    /// 计算下一次出现时间
    pub fn next_occurrence(&self, from: chrono::DateTime<chrono::Utc>) -> Option<chrono::DateTime<chrono::Utc>> {
        let next = match self.frequency {
            RecurrenceFrequency::Daily => from + chrono::Duration::days(self.interval as i64),
            RecurrenceFrequency::Weekly => from + chrono::Duration::weeks(self.interval as i64),
            RecurrenceFrequency::Monthly => {
                from.checked_add_months(chrono::Months::new(self.interval))?
            }
            RecurrenceFrequency::Yearly => {
                from.checked_add_months(chrono::Months::new(self.interval * 12))?
            }
        };

        if let Some(until) = self.until {
            if next > until {
                return None;
            }
        }

        Some(next)
    }
}

/// 习惯追踪
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Habit {
    pub id: String,
    pub title: String,
    pub frequency: HabitFrequency,
    pub streak: u32,
    pub best_streak: u32,
    pub total_completions: u32,
    pub last_completed: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HabitFrequency {
    Daily,
    Weekly { target_days: Vec<chrono::Weekday> },
}

impl Habit {
    pub fn new(title: impl Into<String>, frequency: HabitFrequency) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            title: title.into(),
            frequency,
            streak: 0,
            best_streak: 0,
            total_completions: 0,
            last_completed: None,
            created_at: chrono::Utc::now(),
        }
    }

    pub fn complete(&mut self) {
        let now = chrono::Utc::now();
        self.total_completions += 1;
        self.last_completed = Some(now);

        // 检查连续性
        let should_increment = match &self.frequency {
            HabitFrequency::Daily => {
                match self.last_completed {
                    Some(last) => {
                        let diff = now.date_naive().signed_duration_since(last.date_naive());
                        diff.num_days() <= 1
                    }
                    None => true,
                }
            }
            HabitFrequency::Weekly { .. } => true,
        };

        if should_increment {
            self.streak += 1;
            if self.streak > self.best_streak {
                self.best_streak = self.streak;
            }
        } else {
            self.streak = 1;
        }
    }
}

/// 提醒
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reminder {
    pub id: String,
    pub task_id: Option<TaskId>,
    pub title: String,
    pub remind_at: chrono::DateTime<chrono::Utc>,
    pub dismissed: bool,
}

/// 自动化规则（IFTTT 简化版）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationRule {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub trigger: Trigger,
    pub conditions: Vec<Condition>,
    pub actions: Vec<Action>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Trigger {
    TaskCreated,
    TaskStatusChanged { from: Option<TaskStatus>, to: Option<TaskStatus> },
    TaskDueSoon { hours: u32 },
    TaskOverdue,
    Daily { hour: u32, minute: u32 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Condition {
    StatusIs(TaskStatus),
    PriorityIs(Priority),
    HasTag(String),
    InProject(String),
    DueWithinHours(u32),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    ChangeStatus(TaskStatus),
    SetPriority(Priority),
    AddTag(String),
    MoveToProject(String),
    CreateTask { title: String, status: TaskStatus },
    SendReminder(String),
}

/// GTD 效能引擎
#[derive(Debug, Clone)]
pub struct GtdEngine {
    tasks: Arc<RwLock<HashMap<TaskId, Task>>>,
    projects: Arc<RwLock<HashMap<ProjectId, Project>>>,
    /// Closure table 存储任务层级关系
    task_closure: Arc<RwLock<Vec<ClosureEntry>>>,
    /// Closure table 存储项目层级关系
    project_closure: Arc<RwLock<Vec<ClosureEntry>>>,
    habits: Arc<RwLock<HashMap<String, Habit>>>,
    reminders: Arc<RwLock<Vec<Reminder>>>,
    rules: Arc<RwLock<Vec<AutomationRule>>>,
}

impl Default for GtdEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl GtdEngine {
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(RwLock::new(HashMap::new())),
            projects: Arc::new(RwLock::new(HashMap::new())),
            task_closure: Arc::new(RwLock::new(Vec::new())),
            project_closure: Arc::new(RwLock::new(Vec::new())),
            habits: Arc::new(RwLock::new(HashMap::new())),
            reminders: Arc::new(RwLock::new(Vec::new())),
            rules: Arc::new(RwLock::new(Vec::new())),
        }
    }

    // === 任务管理 ===

    pub fn create_task(&self, task: Task) -> Task {
        let t = task.clone();
        let mut tasks = self.tasks.write();
        let mut closure = self.task_closure.write();

        // 添加自身关系 (ancestor = self, descendant = self, depth = 0)
        closure.push(ClosureEntry {
            ancestor_id: task.id.clone(),
            descendant_id: task.id.clone(),
            depth: 0,
        });

        // 如果有父任务，继承父任务的所有祖先关系
        if let Some(ref parent_id) = task.parent_id {
            let parent_entries: Vec<_> = closure.iter()
                .filter(|e| e.descendant_id == *parent_id)
                .map(|e| ClosureEntry {
                    ancestor_id: e.ancestor_id.clone(),
                    descendant_id: task.id.clone(),
                    depth: e.depth + 1,
                })
                .collect();
            closure.extend(parent_entries);
        }

        tasks.insert(task.id.clone(), task);
        t
    }

    pub fn get_task(&self, task_id: &str) -> Option<Task> {
        self.tasks.read().get(task_id).cloned()
    }

    pub fn update_task(&self, task: Task) -> Option<Task> {
        let mut tasks = self.tasks.write();
        if tasks.contains_key(&task.id) {
            tasks.insert(task.id.clone(), task.clone());
            Some(task)
        } else {
            None
        }
    }

    pub fn delete_task(&self, task_id: &str) -> Option<Task> {
        let mut tasks = self.tasks.write();
        let mut closure = self.task_closure.write();
        let task = tasks.remove(task_id)?;

        // 删除 closure table 中所有相关条目
        closure.retain(|e| e.ancestor_id != task_id && e.descendant_id != task_id);

        Some(task)
    }

    pub fn list_tasks(&self) -> Vec<Task> {
        self.tasks.read().values().cloned().collect()
    }

    pub fn list_tasks_by_status(&self, status: TaskStatus) -> Vec<Task> {
        self.tasks.read()
            .values()
            .filter(|t| t.status == status)
            .cloned()
            .collect()
    }

    pub fn get_inbox(&self) -> Vec<Task> {
        self.list_tasks_by_status(TaskStatus::Inbox)
    }

    /// 批量澄清收件箱任务
    pub fn batch_clarify(&self, task_ids: &[TaskId], new_status: TaskStatus) -> Vec<Task> {
        let mut result = Vec::new();
        let mut tasks = self.tasks.write();

        for id in task_ids {
            if let Some(task) = tasks.get_mut(id) {
                if task.status == TaskStatus::Inbox && TaskStatus::Inbox.can_transition_to(&new_status) {
                    task.status = new_status.clone();
                    task.updated_at = chrono::Utc::now();
                    result.push(task.clone());
                }
            }
        }

        result
    }

    /// 获取子任务（直接子级）
    pub fn get_direct_children(&self, task_id: &str) -> Vec<Task> {
        self.tasks.read()
            .values()
            .filter(|t| t.parent_id.as_ref() == Some(&task_id.to_string()))
            .cloned()
            .collect()
    }

    /// 获取所有后代任务
    pub fn get_all_descendants(&self, task_id: &str) -> Vec<Task> {
        let closure = self.task_closure.read();
        let tasks = self.tasks.read();

        let descendant_ids: HashSet<_> = closure.iter()
            .filter(|e| e.ancestor_id == task_id && e.depth > 0)
            .map(|e| e.descendant_id.clone())
            .collect();

        descendant_ids.into_iter()
            .filter_map(|id| tasks.get(&id).cloned())
            .collect()
    }

    /// 获取任务路径（从根到当前）
    pub fn get_task_path(&self, task_id: &str) -> Vec<Task> {
        let closure = self.task_closure.read();
        let tasks = self.tasks.read();

        let mut path: Vec<_> = closure.iter()
            .filter(|e| e.descendant_id == task_id)
            .map(|e| (e.depth, e.ancestor_id.clone()))
            .collect();

        path.sort_by_key(|(depth, _)| *depth);

        path.into_iter()
            .filter_map(|(_, id)| tasks.get(&id).cloned())
            .collect()
    }

    // === 项目管理 ===

    pub fn create_project(&self, project: Project) -> Project {
        let p = project.clone();
        let mut projects = self.projects.write();
        let mut closure = self.project_closure.write();

        closure.push(ClosureEntry {
            ancestor_id: project.id.clone(),
            descendant_id: project.id.clone(),
            depth: 0,
        });

        if let Some(ref parent_id) = project.parent_id {
            let parent_entries: Vec<_> = closure.iter()
                .filter(|e| e.descendant_id == *parent_id)
                .map(|e| ClosureEntry {
                    ancestor_id: e.ancestor_id.clone(),
                    descendant_id: project.id.clone(),
                    depth: e.depth + 1,
                })
                .collect();
            closure.extend(parent_entries);
        }

        projects.insert(project.id.clone(), project);
        p
    }

    pub fn get_project(&self, project_id: &str) -> Option<Project> {
        self.projects.read().get(project_id).cloned()
    }

    pub fn list_projects(&self) -> Vec<Project> {
        self.projects.read().values().cloned().collect()
    }

    pub fn get_project_tasks(&self, project_id: &str) -> Vec<Task> {
        self.tasks.read()
            .values()
            .filter(|t| t.project_id.as_ref() == Some(&project_id.to_string()))
            .cloned()
            .collect()
    }

    // === 习惯追踪 ===

    pub fn create_habit(&self, habit: Habit) -> Habit {
        let h = habit.clone();
        self.habits.write().insert(habit.id.clone(), habit);
        h
    }

    pub fn complete_habit(&self, habit_id: &str) -> Option<Habit> {
        let mut habits = self.habits.write();
        let habit = habits.get_mut(habit_id)?;
        habit.complete();
        Some(habit.clone())
    }

    pub fn get_habit(&self, habit_id: &str) -> Option<Habit> {
        self.habits.read().get(habit_id).cloned()
    }

    pub fn list_habits(&self) -> Vec<Habit> {
        self.habits.read().values().cloned().collect()
    }

    // === 提醒 ===

    pub fn add_reminder(&self, reminder: Reminder) -> Reminder {
        let r = reminder.clone();
        self.reminders.write().push(reminder);
        r
    }

    pub fn get_pending_reminders(&self) -> Vec<Reminder> {
        let now = chrono::Utc::now();
        self.reminders.read()
            .iter()
            .filter(|r| !r.dismissed && r.remind_at <= now)
            .cloned()
            .collect()
    }

    pub fn dismiss_reminder(&self, reminder_id: &str) -> Option<Reminder> {
        let mut reminders = self.reminders.write();
        let reminder = reminders.iter_mut().find(|r| r.id == reminder_id)?;
        reminder.dismissed = true;
        Some(reminder.clone())
    }

    // === 自动化规则 ===

    pub fn add_rule(&self, rule: AutomationRule) -> AutomationRule {
        let r = rule.clone();
        self.rules.write().push(rule);
        r
    }

    pub fn evaluate_rules_for_task(&self, task: &Task, trigger: &Trigger) -> Vec<Action> {
        let rules = self.rules.read();
        let mut actions = Vec::new();

        for rule in rules.iter().filter(|r| r.enabled) {
            if std::mem::discriminant(&rule.trigger) == std::mem::discriminant(trigger) {
                let all_match = rule.conditions.iter().all(|cond| Self::check_condition(task, cond));
                if all_match {
                    actions.extend(rule.actions.clone());
                }
            }
        }

        actions
    }

    fn check_condition(task: &Task, condition: &Condition) -> bool {
        match condition {
            Condition::StatusIs(s) => task.status == *s,
            Condition::PriorityIs(p) => task.priority == *p,
            Condition::HasTag(tag) => task.tags.contains(tag),
            Condition::InProject(pid) => task.project_id.as_ref() == Some(pid),
            Condition::DueWithinHours(hours) => {
                task.due_date.is_some_and(|due| {
                    let diff = due - chrono::Utc::now();
                    diff.num_hours() <= *hours as i64 && diff.num_hours() >= 0
                })
            }
        }
    }

    pub fn apply_action(&self, task_id: &str, action: &Action) -> Option<Task> {
        let mut tasks = self.tasks.write();
        let task = tasks.get_mut(task_id)?;

        // 仅当动作真正生效时才更新 updated_at：非法状态迁移（被状态机拒绝）
        // 或空操作（未知动作 / 重复标签）此前也会触碰 updated_at 并返回
        // Some，表现为“已应用”但实际未变，属于静默失败。
        let mut changed = false;
        match action {
            Action::ChangeStatus(s) => match task.transition_to(s.clone()) {
                Ok(()) => changed = true,
                Err(e) => {
                    warn!(task_id = %task.id, error = %e, "apply_action ChangeStatus rejected")
                }
            },
            Action::SetPriority(p) => {
                task.priority = p.clone();
                changed = true;
            }
            Action::AddTag(tag) => {
                if !task.tags.contains(tag) {
                    task.tags.push(tag.clone());
                    changed = true;
                }
            }
            Action::MoveToProject(pid) => {
                task.project_id = Some(pid.clone());
                changed = true;
            }
            _ => {}
        }

        if changed {
            task.updated_at = chrono::Utc::now();
        }
        Some(task.clone())
    }

    // === 查询与过滤 ===

    pub fn get_overdue_tasks(&self) -> Vec<Task> {
        self.tasks.read()
            .values()
            .filter(|t| t.is_overdue())
            .cloned()
            .collect()
    }

    pub fn get_tasks_due_soon(&self, hours: i64) -> Vec<Task> {
        let now = chrono::Utc::now();
        let deadline = now + chrono::Duration::hours(hours);

        self.tasks.read()
            .values()
            .filter(|t| {
                t.due_date.is_some_and(|due| {
                    due > now && due <= deadline && t.status != TaskStatus::Done
                })
            })
            .cloned()
            .collect()
    }

    pub fn get_tasks_by_context(&self, context: &str) -> Vec<Task> {
        self.tasks.read()
            .values()
            .filter(|t| t.context.as_ref() == Some(&context.to_string()))
            .cloned()
            .collect()
    }

    pub fn get_tasks_by_tag(&self, tag: &str) -> Vec<Task> {
        self.tasks.read()
            .values()
            .filter(|t| t.tags.contains(&tag.to_string()))
            .cloned()
            .collect()
    }

    /// 获取今日待办（Scheduled + Doing + 即将到期）
    pub fn get_today_tasks(&self) -> Vec<Task> {
        let now = chrono::Utc::now();
        let today_start: chrono::DateTime<chrono::Utc> = now.date_naive()
            .and_hms_opt(0, 0, 0)
            .map(|naive| chrono::DateTime::from_naive_utc_and_offset(naive, chrono::Utc))
            .unwrap_or_else(chrono::Utc::now);
        let today_end = today_start + chrono::Duration::days(1);

        self.tasks.read()
            .values()
            .filter(|t| {
                matches!(t.status, TaskStatus::Scheduled | TaskStatus::Doing)
                    || t.due_date.is_some_and(|due| due >= today_start && due < today_end)
            })
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_lifecycle() {
        let engine = GtdEngine::new();
        let task = Task::new("Buy groceries");
        let task = engine.create_task(task);

        assert_eq!(task.status, TaskStatus::Inbox);

        // 流转到 Clarified
        let mut task = engine.get_task(&task.id).unwrap();
        task.transition_to(TaskStatus::Clarified).unwrap();
        engine.update_task(task.clone());

        let task = engine.get_task(&task.id).unwrap();
        assert_eq!(task.status, TaskStatus::Clarified);

        // 无效流转应该失败
        let mut task = engine.get_task(&task.id).unwrap();
        assert!(task.transition_to(TaskStatus::Done).is_err());
    }

    #[test]
    fn test_task_hierarchy() {
        let engine = GtdEngine::new();

        let parent = engine.create_task(Task::new("Parent Task"));
        let child1 = engine.create_task(Task::new("Child 1").with_parent(&parent.id));
        let child2 = engine.create_task(Task::new("Child 2").with_parent(&child1.id));

        let direct_children = engine.get_direct_children(&parent.id);
        assert_eq!(direct_children.len(), 1);
        assert_eq!(direct_children[0].id, child1.id);

        let all_descendants = engine.get_all_descendants(&parent.id);
        assert_eq!(all_descendants.len(), 2);

        let path = engine.get_task_path(&child2.id);
        assert_eq!(path.len(), 3); // parent -> child1 -> child2
    }

    #[test]
    fn test_inbox_and_clarify() {
        let engine = GtdEngine::new();

        let t1 = engine.create_task(Task::new("Idea 1"));
        let t2 = engine.create_task(Task::new("Idea 2"));
        let t3 = engine.create_task(Task::new("Idea 3"));

        let inbox = engine.get_inbox();
        assert_eq!(inbox.len(), 3);

        // 批量澄清前两个
        engine.batch_clarify(&[t1.id.clone(), t2.id.clone()], TaskStatus::Clarified);

        let inbox = engine.get_inbox();
        assert_eq!(inbox.len(), 1);
        assert_eq!(inbox[0].id, t3.id);
    }

    #[test]
    fn test_project_tasks() {
        let engine = GtdEngine::new();

        let project = engine.create_project(Project::new("Website Redesign"));
        let task = engine.create_task(
            Task::new("Design mockups").with_project(&project.id)
        );

        let project_tasks = engine.get_project_tasks(&project.id);
        assert_eq!(project_tasks.len(), 1);
        assert_eq!(project_tasks[0].id, task.id);
    }

    #[test]
    fn test_overdue_detection() {
        let engine = GtdEngine::new();

        let overdue = engine.create_task(
            Task::new("Overdue task")
                .with_due_date(chrono::Utc::now() - chrono::Duration::days(1))
        );

        let future = engine.create_task(
            Task::new("Future task")
                .with_due_date(chrono::Utc::now() + chrono::Duration::days(1))
        );

        assert!(overdue.is_overdue());
        assert!(!future.is_overdue());

        let overdue_tasks = engine.get_overdue_tasks();
        assert_eq!(overdue_tasks.len(), 1);
        assert_eq!(overdue_tasks[0].id, overdue.id);
    }

    #[test]
    fn test_habit_streak() {
        let engine = GtdEngine::new();

        let habit = Habit::new("Read 30 min", HabitFrequency::Daily);
        let habit = engine.create_habit(habit);

        assert_eq!(habit.streak, 0);

        engine.complete_habit(&habit.id);
        let habit = engine.get_habit(&habit.id).unwrap();
        assert_eq!(habit.streak, 1);
        assert_eq!(habit.total_completions, 1);

        engine.complete_habit(&habit.id);
        let habit = engine.get_habit(&habit.id).unwrap();
        assert_eq!(habit.streak, 2);
        assert_eq!(habit.best_streak, 2);
    }

    #[test]
    fn test_automation_rule() {
        let engine = GtdEngine::new();

        let rule = AutomationRule {
            id: Uuid::new_v4().to_string(),
            name: "Auto-tag urgent".to_string(),
            enabled: true,
            trigger: Trigger::TaskCreated,
            conditions: vec![Condition::PriorityIs(Priority::Urgent)],
            actions: vec![
                Action::AddTag("needs-attention".to_string()),
                Action::ChangeStatus(TaskStatus::Organized),
            ],
        };
        engine.add_rule(rule);

        let urgent_task = Task::new("Critical bug").with_priority(Priority::Urgent);
        let actions = engine.evaluate_rules_for_task(&urgent_task, &Trigger::TaskCreated);

        assert_eq!(actions.len(), 2);
    }

    #[test]
    fn test_recurrence_rule() {
        let rule = RecurrenceRule {
            frequency: RecurrenceFrequency::Daily,
            interval: 1,
            count: None,
            until: None,
            by_weekday: None,
        };

        let now = chrono::Utc::now();
        let next = rule.next_occurrence(now).unwrap();
        assert_eq!((next - now).num_days(), 1);

        let weekly = RecurrenceRule {
            frequency: RecurrenceFrequency::Weekly,
            interval: 1,
            count: None,
            until: None,
            by_weekday: None,
        };
        let next = weekly.next_occurrence(now).unwrap();
        assert_eq!((next - now).num_weeks(), 1);
    }

    #[test]
    fn test_today_tasks() {
        let engine = GtdEngine::new();

        engine.create_task(Task::new("Scheduled task").with_priority(Priority::High));
        let mut doing = Task::new("Doing task");
        doing.status = TaskStatus::Doing;
        engine.create_task(doing);

        let today = engine.get_today_tasks();
        assert_eq!(today.len(), 1); // only Doing task (Scheduled has default Inbox status in this test)
    }

    #[test]
    fn test_task_filtering() {
        let engine = GtdEngine::new();

        engine.create_task(
            Task::new("Task 1")
                .with_tags(vec!["work".to_string(), "urgent".to_string()])
                .with_context("office")
        );
        engine.create_task(
            Task::new("Task 2")
                .with_tags(vec!["personal".to_string()])
                .with_context("home")
        );

        let work_tasks = engine.get_tasks_by_tag("work");
        assert_eq!(work_tasks.len(), 1);

        let office_tasks = engine.get_tasks_by_context("office");
        assert_eq!(office_tasks.len(), 1);
    }
}