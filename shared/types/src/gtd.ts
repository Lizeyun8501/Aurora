/**
 * GTD productivity system domain types.
 * Mirrors `crates/aurora-core/src/l3_domain/gtd_system.rs`.
 */

/** Task unique identifier (mirrors `TaskId`). */
export type TaskId = string;

/** Project unique identifier (mirrors `ProjectId`). */
export type ProjectId = string;

/** Task status (mirrors `TaskStatus`, serde `snake_case`). */
export type TaskStatus =
  | 'inbox'
  | 'clarified'
  | 'organized'
  | 'scheduled'
  | 'doing'
  | 'done'
  | 'archived';

/** Priority (mirrors `Priority`, serde `snake_case`). */
export type Priority = 'low' | 'medium' | 'high' | 'urgent';

/** Energy level (mirrors `EnergyLevel`, serde `snake_case`). */
export type EnergyLevel = 'low' | 'medium' | 'high';

/** ISO weekday (1 = Monday ... 7 = Sunday), mirroring `chrono::Weekday`. */
export type Weekday = 1 | 2 | 3 | 4 | 5 | 6 | 7;

/** Task structure (mirrors `Task`). */
export interface Task {
  id: TaskId;
  title: string;
  description: string | null;
  status: TaskStatus;
  priority: Priority;
  project_id: ProjectId | null;
  parent_id: TaskId | null;
  due_date: string | null;
  scheduled_date: string | null;
  completed_at: string | null;
  tags: string[];
  context: string | null;
  estimated_minutes: number | null;
  created_at: string;
  updated_at: string;
  energy_level: EnergyLevel | null;
}

/** Project status (mirrors `ProjectStatus`, serde `snake_case`). */
export type ProjectStatus = 'active' | 'on_hold' | 'completed' | 'cancelled';

/** Project structure (mirrors `Project`). */
export interface Project {
  id: ProjectId;
  title: string;
  description: string | null;
  goal: string | null;
  status: ProjectStatus;
  parent_id: ProjectId | null;
  tags: string[];
  created_at: string;
  updated_at: string;
}

/** Closure-table hierarchy entry (mirrors `ClosureEntry`). */
export interface ClosureEntry {
  ancestor_id: string;
  descendant_id: string;
  depth: number;
}

/** Recurrence frequency (mirrors `RecurrenceFrequency`, serde `snake_case`). */
export type RecurrenceFrequency = 'daily' | 'weekly' | 'monthly' | 'yearly';

/** Recurrence rule — simplified RRULE (mirrors `RecurrenceRule`). */
export interface Rrule {
  frequency: RecurrenceFrequency;
  interval: number;
  count: number | null;
  until: string | null;
  by_weekday: Weekday[] | null;
}

/** Habit frequency (mirrors `HabitFrequency`). */
export type HabitFrequency =
  | { kind: 'daily' }
  | { kind: 'weekly'; target_days: Weekday[] };

/** Habit structure (mirrors `Habit`). */
export interface Habit {
  id: string;
  title: string;
  frequency: HabitFrequency;
  streak: number;
  best_streak: number;
  total_completions: number;
  last_completed: string | null;
  created_at: string;
}

/** A single habit completion entry (habit check-in log). */
export interface HabitEntry {
  id: string;
  habit_id: string;
  completed_at: string;
  note: string | null;
}

/** Reminder (mirrors `Reminder`). */
export interface Reminder {
  id: string;
  task_id: TaskId | null;
  title: string;
  remind_at: string;
  dismissed: boolean;
}

/** Automation trigger (mirrors `Trigger`). */
export type Trigger =
  | { kind: 'task_created' }
  | { kind: 'task_status_changed'; from: TaskStatus | null; to: TaskStatus | null }
  | { kind: 'task_due_soon'; hours: number }
  | { kind: 'task_overdue' }
  | { kind: 'daily'; hour: number; minute: number };

/** Automation condition (mirrors `Condition`). */
export type Condition =
  | { kind: 'status_is'; status: TaskStatus }
  | { kind: 'priority_is'; priority: Priority }
  | { kind: 'has_tag'; tag: string }
  | { kind: 'in_project'; project_id: string }
  | { kind: 'due_within_hours'; hours: number };

/** Automation action (mirrors `Action`). */
export type Action =
  | { kind: 'change_status'; status: TaskStatus }
  | { kind: 'set_priority'; priority: Priority }
  | { kind: 'add_tag'; tag: string }
  | { kind: 'move_to_project'; project_id: string }
  | { kind: 'create_task'; title: string; status: TaskStatus }
  | { kind: 'send_reminder'; message: string };

/** Automation rule — IFTTT-style (mirrors `AutomationRule`). */
export interface AutomationRule {
  id: string;
  name: string;
  enabled: boolean;
  trigger: Trigger;
  conditions: Condition[];
  actions: Action[];
}
