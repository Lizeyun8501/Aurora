import { type ReactElement } from 'react';
import type { TimelineItem, TodayViewData } from '@aurora/shared-types';
import clsx from 'clsx';

export interface TodayViewProps {
  data: TodayViewData;
  className?: string;
}

interface TimelineEntry extends TimelineItem {}

/** Builds a combined timeline from today's events and due todos. */
function buildTimeline(data: TodayViewData): TimelineEntry[] {
  const items: TimelineEntry[] = data.events.map((e) => ({
    id: e.id,
    title: e.title,
    start: e.start,
    end: e.end,
    kind: 'event',
    source_id: e.id,
  }));
  for (const t of data.todos) {
    if (t.due_at) {
      items.push({
        id: t.id,
        title: t.title,
        start: t.due_at,
        end: null,
        kind: 'todo',
        source_id: t.id,
      });
    }
  }
  return items.sort((a, b) => a.start.localeCompare(b.start));
}

/** Renders the "Today" dashboard: timeline, focus stats, habits, daily report. */
export function TodayView({ data, className }: TodayViewProps): ReactElement {
  const timeline = buildTimeline(data);
  const completed = data.todos.filter((t) => t.completed).length;
  const total = data.todos.length;
  const rate = total > 0 ? Math.round((completed / total) * 100) : 0;
  const focus = data.focus_stats;

  return (
    <section className={clsx('aurora-today-view', className)} aria-label="Today view">
      <header className="aurora-today-header">
        <h2>Today</h2>
        <time dateTime={data.date}>{data.date}</time>
      </header>

      <div className="aurora-today-timeline">
        <h3>Timeline</h3>
        {timeline.length === 0 ? (
          <p className="aurora-today-empty">No scheduled items.</p>
        ) : (
          <ul>
            {timeline.map((item) => (
              <li key={item.id} className={`aurora-timeline-item kind-${item.kind}`}>
                <span className="aurora-timeline-time">{item.start}</span>
                <span className="aurora-timeline-title">{item.title}</span>
              </li>
            ))}
          </ul>
        )}
      </div>

      <div className="aurora-today-focus-card">
        <h3>Focus</h3>
        {focus ? (
          <dl>
            <dt>Sessions</dt>
            <dd>{focus.completed_sessions}/{focus.total_sessions}</dd>
            <dt>Focus minutes</dt>
            <dd>{focus.total_focus_minutes}</dd>
            <dt>Longest streak</dt>
            <dd>{focus.longest_streak}</dd>
          </dl>
        ) : (
          <p className="aurora-today-empty">No focus data yet.</p>
        )}
      </div>

      <div className="aurora-today-habits">
        <h3>Habits</h3>
        {data.habits.length === 0 ? (
          <p className="aurora-today-empty">No habits tracked.</p>
        ) : (
          <ul>
            {data.habits.map((h) => (
              <li
                key={h.id}
                className={clsx('aurora-habit', h.completed && 'completed')}
              >
                <span>{h.name}</span>
                <span className="aurora-habit-streak">🔥 {h.streak}</span>
              </li>
            ))}
          </ul>
        )}
      </div>

      <div className="aurora-today-report-preview">
        <h3>Daily report preview</h3>
        <p>
          {completed}/{total} tasks completed ({rate}%)
        </p>
      </div>
    </section>
  );
}
