/**
 * Today View domain types.
 * Mirrors `crates/aurora-core/src/l3_domain/today_view.rs`.
 */

/** Timeline granularity (mirrors `TimelineGranularity`, serde `snake_case`). */
export type TimelineGranularity = 'day' | 'week' | 'month';

/** Timeline item kind (mirrors `TimelineItemKind`, serde `snake_case`). */
export type TimelineItemKind = 'event' | 'todo' | 'focus_session' | 'habit';

/** Pomodoro phase (mirrors `PomodoroPhase`, serde `snake_case`). */
export type PomodoroPhase = 'work' | 'break';

/** White-noise kind (mirrors `WhiteNoiseKind`, serde `snake_case`). */
export type WhiteNoiseKind = 'none' | 'rain' | 'forest' | 'brown_noise' | 'ocean';

/** A today-view todo (mirrors `TodayTodo`). */
export interface TodayTodo {
  id: string;
  title: string;
  due_at: string | null;
  priority: number;
  completed: boolean;
  estimated_minutes: number | null;
}

/** A today-view calendar event (mirrors `TodayEvent`). */
export interface TodayEvent {
  id: string;
  title: string;
  start: string;
  end: string;
  location: string | null;
}

/** A today-view habit check-in (mirrors `TodayHabit`). */
export interface TodayHabit {
  id: string;
  name: string;
  completed: boolean;
  /** Current streak (days). */
  streak: number;
}

/** Focus statistics (mirrors `FocusStats`). */
export interface FocusStats {
  total_sessions: number;
  completed_sessions: number;
  total_focus_minutes: number;
  longest_streak: number;
}

/** Today view aggregate data (mirrors `TodayViewData`). */
export interface TodayViewData {
  /** Calendar date (YYYY-MM-DD). */
  date: string;
  todos: TodayTodo[];
  events: TodayEvent[];
  habits: TodayHabit[];
  focus_stats: FocusStats | null;
  generated_at: string;
}

/** A timeline item (mirrors `TimelineItem`). */
export interface TimelineItem {
  id: string;
  title: string;
  start: string;
  end: string | null;
  kind: TimelineItemKind;
  source_id: string;
}

/** Timeline view (mirrors `TimelineView`, supports virtualized slicing). */
export interface TimelineView {
  granularity: TimelineGranularity;
  start: string;
  end: string;
  items: TimelineItem[];
  total: number;
}

/** A focus session (mirrors `FocusSession`). */
export interface FocusSession {
  id: string;
  started_at: string;
  ended_at: string | null;
  planned_duration_minutes: number;
  actual_duration_minutes: number | null;
  completed: boolean;
  task_id: string | null;
  white_noise: WhiteNoiseKind;
}

/** Pomodoro timer state (mirrors `PomodoroState`). */
export interface PomodoroState {
  phase: PomodoroPhase;
  remaining_seconds: number;
  completed_work_cycles: number;
  running: boolean;
}

/** Daily report (mirrors `DailyReport`). */
export interface DailyReport {
  /** Calendar date (YYYY-MM-DD). */
  date: string;
  task_completion_rate: number;
  completed_tasks: number;
  total_tasks: number;
  time_allocation_minutes: number;
  habit_continuity: number;
  focus_sessions: number;
  highlights: string[];
}

/** Review history (mirrors `ReviewHistory`). */
export interface ReviewHistory {
  reports: DailyReport[];
}
