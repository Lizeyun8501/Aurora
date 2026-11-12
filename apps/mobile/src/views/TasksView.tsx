/**
 * TasksView — B-02 切片二: 从 MobileApp.tsx 拆出（纯搬移零逻辑改动）。
 * 壳保留导航/状态注入/路由;视图自持渲染（视图=布局与胶水语义）。
 */
import React, { useEffect, useMemo, useRef, useState, useCallback } from 'react';
import { platform } from '../adapters/androidPlatform';
import { I, NoteCard, EmptyState, relativeTime, type Task, type TaskPriority, type TaskStatus, TASK_PRIO, TASK_GROUPS } from '../components/primitives';

export function TasksView({ tasks, onTasks, showToast, onOpenTask }: {
  tasks: Task[];
  onTasks: (ts: Task[]) => void;
  showToast: (m: string) => void;
  /** 11-08 23:10 用户令: 单击已有待办→跳转编辑（日历对应日 day 页签）。 */
  onOpenTask?: (t: Task) => void;
}) {
  const [filter, setFilter] = useState<'all' | 'open' | 'done'>('all');
  const [prioFilter, setPrioFilter] = useState<'all' | TaskPriority>('all');
  /* 11-08 23:10 用户令: 长按 550ms 删除——touchmove(滑删手势)即取消;触发后置 fired 抑制松手 click 跳转。 */
  const lp = useRef<{ timer: ReturnType<typeof setTimeout> | null; fired: boolean }>({ timer: null, fired: false });
  const open = tasks.filter((t) => t.status !== 'done');
  const done = tasks.filter((t) => t.status === 'done');
  const base = filter === 'all' ? tasks : filter === 'open' ? open : done;
  // E17: 优先级过滤+排序权重（high>normal>low>无;同级 due 近者前）
  const PRIO_W: Record<string, number> = { high: 0, normal: 1, low: 2 };
  const shown = (prioFilter === 'all' ? base : base.filter((t) => (t.priority ?? 'normal') === prioFilter))
    .slice().sort((a, b) => {
      const w = (PRIO_W[a.priority ?? 'normal'] ?? 1) - (PRIO_W[b.priority ?? 'normal'] ?? 1);
      if (w !== 0) return w;
      return (a.due ?? '9999').localeCompare(b.due ?? '9999');
    });

  const toggle = (id: string) =>
    onTasks(tasks.map((t) => t.id === id
      ? { ...t, status: t.status === 'done' ? 'next' : 'done' } : t));

  const cyclePrio = (id: string) =>
    onTasks(tasks.map((t) => {
      if (t.id !== id) return t;
      const order: TaskPriority[] = ['low', 'normal', 'high'];
      const cur = order.indexOf(t.priority ?? 'normal');
      return { ...t, priority: order[(cur + 1) % 3] };
    }));

  const remove = (id: string) => {
    onTasks(tasks.filter((t) => t.id !== id));
    showToast('任务已删除');
  };

  // E17: 日期分组（逾期/今日/明日/本周/之后/无日期）
  const todayStr = (() => { const n = new Date(); return `${n.getFullYear()}-${String(n.getMonth() + 1).padStart(2, '0')}-${String(n.getDate()).padStart(2, '0')}`; })();
  const addDays = (n: number) => { const d = new Date(); d.setDate(d.getDate() + n); return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`; };
  const tomorrowStr = addDays(1);
  const weekEndStr = addDays(7);
  const bucketOf = (t: Task): { key: string; label: string; danger?: boolean } => {
    if (!t.due) return { key: 'nodue', label: '稍后安排' };
    if (t.due < todayStr) return { key: 'overdue', label: '已逾期', danger: true };
    if (t.due === todayStr) return { key: 'today', label: '今日' };
    if (t.due === tomorrowStr) return { key: 'tomorrow', label: '明日' };
    if (t.due <= weekEndStr) return { key: 'week', label: '本周' };
    return { key: 'later', label: '之后' };
  };
  const GROUP_ORDER = ['overdue', 'today', 'tomorrow', 'week', 'later', 'nodue'];
  const grouped = new Map<string, Task[]>();
  for (const t of shown) {
    const b = bucketOf(t).key;
    if (!grouped.has(b)) grouped.set(b, []);
    grouped.get(b)!.push(t);
  }

  // E17: 统计头（完成率环+今日量+逾期量）
  const total = tasks.length;
  const donePct = total ? Math.round((done.length / total) * 100) : 0;
  const todayCount = tasks.filter((t) => t.due === todayStr && t.status !== 'done').length;
  const overdueCount = tasks.filter((t) => t.due && t.due < todayStr && t.status !== 'done').length;
  const highOpen = open.filter((t) => (t.priority ?? 'normal') === 'high').length;

  return (
    <div className="view">
      {/* E17 统计头: 完成率+今日+逾期+高优（一屏知全局） */}
      <div className="task-stats-row">
        <div className="task-stat" style={{ flex: '0 0 auto' }}>
          <div className="task-stat-ring" style={{ background: `conic-gradient(var(--success) ${donePct * 3.6}deg, var(--bg-secondary) 0)` }}>
            <span className="task-stat-ring-num">{donePct}%</span>
          </div>
        </div>
        <div className="task-stat"><span className="task-stat-num">{todayCount}</span><span className="task-stat-label">今日</span></div>
        <div className="task-stat"><span className="task-stat-num" style={{ color: overdueCount ? 'var(--danger)' : undefined }}>{overdueCount}</span><span className="task-stat-label">逾期</span></div>
        <div className="task-stat"><span className="task-stat-num" style={{ color: highOpen ? 'var(--danger)' : undefined }}>{highOpen}</span><span className="task-stat-label">高优先</span></div>
      </div>

      <div className="chip-row">
        <button className={`chip ${filter === 'all' ? 'active' : ''}`} onClick={() => setFilter('all')}>
          全部 {tasks.length}
        </button>
        <button className={`chip ${filter === 'open' ? 'active' : ''}`} onClick={() => setFilter('open')}>
          进行中 {open.length}
        </button>
        <button className={`chip ${filter === 'done' ? 'active' : ''}`} onClick={() => setFilter('done')}>
          已完成 {done.length}
        </button>
        <span className="chip-row-sep" />
        {/* E17: 优先级过滤段 */}
        {TASK_PRIO.map((p) => (
          <button key={p.id} className={`chip ${prioFilter === p.id ? 'active' : ''}`}
            style={prioFilter === p.id ? { background: p.dot, color: '#fff', borderColor: p.dot } : undefined}
            onClick={() => setPrioFilter(prioFilter === p.id ? 'all' : p.id)}>
            {p.label} {base.filter((t) => (t.priority ?? 'normal') === p.id).length}
          </button>
        ))}
      </div>

      {shown.length === 0 ? (
        <EmptyState text="暂无任务。点右下角 + 捕获，加入今日任务。" />
      ) : (
        /* E17: 日期分组渲染（逾期/今日/明日/本周/之后/稍后——组头计数+逾期红头） */
        GROUP_ORDER.filter((k) => grouped.has(k)).map((k) => {
          const list = grouped.get(k)!;
          const b = bucketOf(list[0]);
          return (
            <div key={k} className="task-group">
              <div className={`task-group-head ${b.danger ? 'danger' : ''}`}>
                <span>{b.label}</span><span className="task-group-count">{list.length}</span>
              </div>
              <div className="card" style={{ padding: 4 }}>
                {list.map((t) => {
                  const subs = tasks.filter((st) => st.parentTaskId === t.id);
                  const subDone = subs.filter((st) => st.status === 'done').length;
                  return (
                    <div key={t.id} className="task-item"
                      /* 11-08 23:10 用户令: 单击=跳转编辑(日历对应日 day 页签),长按 550ms=删除。
                         touchmove 立即取消长按(让位滑动/滚动手势);长按触发后 fired 抑制松手 click 双动作。 */
                      onClick={() => { if (lp.current.fired) { lp.current.fired = false; return; } onOpenTask?.(t); }}
                      onTouchStart={() => { lp.current.fired = false; lp.current.timer = setTimeout(() => { lp.current.fired = true; lp.current.timer = null; remove(t.id); }, 550); }}
                      onTouchMove={() => { if (lp.current.timer) { clearTimeout(lp.current.timer); lp.current.timer = null; } }}
                      onTouchEnd={() => { if (lp.current.timer) { clearTimeout(lp.current.timer); lp.current.timer = null; } }}
                    >
                      <button
                        className={`task-check ${t.status === 'done' ? 'checked' : ''}`}
                        onClick={(e) => { e.stopPropagation(); toggle(t.id); }}
                        aria-label="完成"
                      >{I.check}</button>
                      {/* E17: 优先级色点（点击循环 low→normal→high） */}
                      <button className="task-prio-dot-btn" aria-label="优先级" title="切换优先级"
                        onClick={(e) => { e.stopPropagation(); cyclePrio(t.id); }}>
                        <span className="task-prio-dot" style={{ background: TASK_PRIO.find((p) => p.id === (t.priority ?? 'normal'))?.dot }} />
                      </button>
                      <div className="task-body">
                        <div className="task-text">{t.text}</div>
                        <div className="task-meta">
                          <span className="task-status-tag">{TASK_GROUPS.find((g) => g.id === t.status)?.label}</span>
                          {t.due && <span className={`task-due ${t.due < todayStr && t.status !== 'done' ? 'overdue' : ''}`}>{t.due}</span>}
                          {subs.length > 0 && <span className="task-sub-prog">{subDone}/{subs.length}</span>}
                          <span className="task-meta-time">{relativeTime(new Date(t.createdAt).toISOString())}</span>
                        </div>
                      </div>
                      <button className="icon-btn" onClick={(e) => { e.stopPropagation(); remove(t.id); }} aria-label="删除任务">{I.x}</button>
                    </div>
                  );
                })}
              </div>
            </div>
          );
        })
      )}

      {done.length > 0 && (
        <button
          className="btn btn-text btn-block"
          style={{ marginTop: 8 }}
          onClick={() => { onTasks(open); showToast('已清除完成任务'); }}
        >
          清除全部已完成
        </button>
      )}
    </div>
  );
}
