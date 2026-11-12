/**
 * TodayView — B-02 切片三: 从 MobileApp 拆出（纯搬移零逻辑改动）。
 */
import React, { useEffect, useMemo, useRef, useState } from 'react';
import { platform, type CalEvent } from '../adapters/androidPlatform';
import { I, EmptyState, relativeTime, listNotesTitled, type Task, type TaskStatus, TASK_PRIO, TASK_GROUPS, Switch } from '../components/primitives';
import { expandAll } from './repeatUtil';
import { VivoIcon, FocusBadge } from '../components/vivo';

export function TodayView({ tasks, events, onTasks, onOpenNote, showToast, onOpenDate }: {
  tasks: Task[];
  /** DK-CALENDAR-04 任务 6: 今日事件块进「日程」分区（Event 按 start 落——与 Task 并列）。 */
  events: CalEvent[];
  onTasks: (ts: Task[]) => void;
  onOpenNote: (id: string, title: string) => void;
  showToast: (t: string) => void;
  /** 2026-10-28 周条可点选日——跳日历页选中该日（用户实测「本周内无法点击」反馈） */
  onOpenDate: (dateStr: string) => void;
}) {
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({ done: true });
  const [pomoRunning, setPomoRunning] = useState(false);
  const [pomoSec, setPomoSec] = useState(25 * 60);

  // 番茄钟 — 计时纯减法，完成判定移出 updater（副作用归 effect）
  useEffect(() => {
    if (!pomoRunning) return;
    const t = setInterval(() => setPomoSec((s) => Math.max(0, s - 1)), 1000);
    return () => clearInterval(t);
  }, [pomoRunning]);

  useEffect(() => {
    if (pomoRunning && pomoSec === 0) {
      setPomoRunning(false);
      setPomoSec(25 * 60);
      showToast('番茄钟完成 🍅');
    }
  }, [pomoRunning, pomoSec, showToast]);

  const today = new Date();
  const dateStr = `${today.getMonth() + 1} 月 ${today.getDate()} 日`;
  const weekDay = ['日', '一', '二', '三', '四', '五', '六'][today.getDay()];
  const doneToday = tasks.filter((t) => t.status === 'done').length;
  const openCount = tasks.filter((t) => t.status !== 'done').length;

  const notes = listNotesTitled();
  const weekNotes = notes.slice(0, 3);

  const toggle = (id: string) => {
    onTasks(tasks.map((t) =>
      t.id === id ? { ...t, status: t.status === 'done' ? 'next' : 'done' } : t));
  };

  // 左滑删除
  const swipe = useRef<{ x: number; id: string | null }>({ x: 0, id: null });

  return (
    <div className="view">
      <header className="today-header">
        <div className="today-date">{dateStr} · 周{weekDay}</div>
        <div className="today-sub">
          <span className="sync-badge"><span className="sync-dot ok" />已同步</span>
          <span>{openCount} 项待办 · {doneToday} 项已完成</span>
          <FocusBadge tasks={tasks} />
        </div>
      </header>

      {/* AI 今日洞察 — 本地统计摘要（无 AI 后端时诚实降级） */}
      <div className="card insight-card">
        <span className="insight-icon">{I.spark}</span>
        <div className="insight-text">
          今日已完成 <b>{doneToday}</b> 项任务，待办 <b>{openCount}</b> 项。
          本周更新 {notes.length} 篇笔记。
          <span style={{ color: 'var(--text-tertiary)' }}>（本地统计 · Agent 未启用）</span>
        </div>
      </div>

      {/* DK-CALENDAR-04 任务 6: 日程分区——今日事件块（展开实例;与 Task 并列——PRD 今日备忘 P0）。 */}
      {(() => {
        const todayKey = (() => {
          const n = new Date();
          return `${n.getFullYear()}-${String(n.getMonth() + 1).padStart(2, '0')}-${String(n.getDate()).padStart(2, '0')}`;
        })();
        const insts = expandAll(events, new Date(`${todayKey}T00:00:00`), new Date(`${todayKey}T23:59:59`));
        insts.sort((a, b) => a.start.getTime() - b.start.getTime());
        if (insts.length === 0) return null;
        return (
          <div className="task-section open">
            <button className="task-section-head" aria-label="日程分区">
              <span>今日日程</span><span className="count">{insts.length}</span>
            </button>
            <div className="ev-list-group">
              {insts.map((inst) => (
                <button key={`${inst.ev.id}@${inst.offsetDays}`} className="ev-list-item" role="button"
                  onClick={() => onOpenDate(todayKey)}>
                  <i className="ev-list-dot" style={{ background: ['#dc2626', '#2563eb', '#059669', '#d97706', '#7c3aed', '#6b7280'][inst.ev.color % 6] }} />
                  <span className="ev-list-time">{inst.ev.all_day ? '全天' : `${String(inst.start.getHours()).padStart(2, '0')}:${String(inst.start.getMinutes()).padStart(2, '0')}`}</span>
                  <span className="ev-list-title">{inst.ev.title}</span>
                </button>
              ))}
            </div>
          </div>
        );
      })()}

      {/* 任务分区 — GTD2.0 四组可折叠 */}
      {TASK_GROUPS.map((g) => {
        const items = tasks.filter((t) => t.status === g.id);
        if (g.id !== 'done' && items.length === 0) {
          return (
            <div key={g.id} className="task-section collapsed">
              <button className="task-section-head" onClick={() => setCollapsed({ ...collapsed, [g.id]: !collapsed[g.id] })}>
                <span>{g.label}</span><span className="count">空</span><span className="chev">{I.chev}</span>
              </button>
            </div>
          );
        }
        return (
          <div key={g.id} className={`task-section ${collapsed[g.id] ? 'collapsed' : 'open'}`}>
            <button className="task-section-head" onClick={() => setCollapsed({ ...collapsed, [g.id]: !collapsed[g.id] })}>
              <span>{g.label}</span>
              <span className="count">{items.length} 项</span>
              <span className="chev">{I.chev}</span>
            </button>
            <div className="task-section-body">
              {items.map((t) => (
                <div
                  key={t.id}
                  className={`task-item ${t.status === 'done' ? 'done' : ''}`}
                  onTouchStart={(e) => { swipe.current = { x: e.touches[0].clientX, id: t.id }; }}
                  onTouchMove={(e) => {
                    const dx = e.touches[0].clientX - swipe.current.x;
                    if (swipe.current.id === t.id && dx < -60) swipe.current.id = `del:${t.id}`;
                  }}
                  onTouchEnd={() => {
                    if (swipe.current.id === `del:${t.id}`) {
                      onTasks(tasks.filter((x) => x.id !== t.id));
                      showToast('任务已删除');
                    }
                    swipe.current = { x: 0, id: null };
                  }}
                >
                  <button
                    className={`task-check ${t.status === 'done' ? 'checked' : ''}`}
                    onClick={() => toggle(t.id)}
                    aria-label="完成任务"
                  >{I.check}</button>
                  <div className="task-body">
                    <div className="task-text">{t.text}</div>
                    <div className="task-meta">
                      {t.due && <span className="task-due">{t.due}</span>}
                      {t.noteId && (
                        <button className="task-note-link" onClick={() => {
                          const n = listNotesTitled().find((x) => x.id === t.noteId);
                          if (n) onOpenNote(n.id, n.title);
                        }}>{I.note} 关联笔记</button>
                      )}
                    </div>
                  </div>
                </div>
              ))}
            </div>
          </div>
        );
      })}

      {/* 本周日历 — 2026-10-28 可点选日（原静态展示 div，用户实测点击无反应） */}
      <div className="card">
        <h3 className="card-title">本周</h3>
        <div className="week-calendar">
          {Array.from({ length: 7 }, (_, i) => {
            const d = new Date(today);
            d.setDate(d.getDate() - d.getDay() + i + 1); // 周一起
            const isToday = d.toDateString() === today.toDateString();
            const ds = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
            return (
              <button
                key={i}
                type="button"
                className={`week-day ${isToday ? 'today' : ''}`}
                onClick={() => onOpenDate(ds)}
                aria-label={`查看 ${ds}`}
              >
                <span>{['一', '二', '三', '四', '五', '六', '日'][i]}</span>
                <span className="d">{d.getDate()}</span>
                <span className="week-dot has" />
              </button>
            );
          })}
        </div>
      </div>

      {/* 重点笔记 */}
      {weekNotes.length > 0 && (
        <div className="card">
          <h3 className="card-title">最近笔记</h3>
          {weekNotes.map((n) => (
            <div key={n.id} className="mini-note" onClick={() => onOpenNote(n.id, n.title)}>
              <span className="mini-note-title">{n.title}</span>
              <span className="mini-note-time">{relativeTime(n.updatedAt)}</span>
            </div>
          ))}
        </div>
      )}

      {/* 番茄钟 */}
      <div className="card">
        <h3 className="card-title">专注</h3>
        <div className="pomodoro">
          <div className={`pomo-time ${pomoRunning ? 'running' : ''}`}>
            {String(Math.floor(pomoSec / 60)).padStart(2, '0')}:{String(pomoSec % 60).padStart(2, '0')}
          </div>
          <div className="pomo-btns">
            <button className="btn btn-primary" onClick={() => setPomoRunning(!pomoRunning)}>
              {pomoRunning ? '暂停' : '开始专注'}
            </button>
            <button className="btn btn-secondary" onClick={() => { setPomoRunning(false); setPomoSec(25 * 60); }}>
              重置
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
