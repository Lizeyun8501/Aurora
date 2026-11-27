/**
 * TasksView — B-02 切片二: 从 MobileApp.tsx 拆出（纯搬移零逻辑改动）。
 * 壳保留导航/状态注入/路由;视图自持渲染（视图=布局与胶水语义）。
 */
import React, { useEffect, useMemo, useRef, useState, useCallback } from 'react';
import { platform } from '../adapters/androidPlatform';
import {I, NoteCard, relativeTime, type Task, type TaskPriority, type TaskStatus, TASK_PRIO, TASK_GROUPS} from '../components/primitives';
import { EmptyState, useSwipeRow } from '@aurora/ui-components';

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
  /* F-03: 依赖管理 sheet（任务详情前置任务区块——可搜添加/移除+环提示）。 */
  const [depSheet, setDepSheet] = useState<{ taskId: string } | null>(null);
  const [depPick, setDepPick] = useState('');
  const [depErr, setDepErr] = useState<string | null>(null);
  // deps 形: taskId → 前置 id 列表（真桥 JSON / mock localStorage 同源）
  const [depsMap, setDepsMap] = useState<Record<string, string[]>>({});

  // F-03: 挂载时全量拉一次各任务前置（open 任务列表——量级有限）
  useEffect(() => {
    const next: Record<string, string[]> = {};
    for (const t of tasks) {
      if (t.status === 'done') continue;
      try {
        const ids = JSON.parse(platform.taskGetDependencies(t.id)) as string[];
        if (Array.isArray(ids) && ids.length > 0) next[t.id] = ids;
      } catch { /* 桥缺省——mock 链自会回 [] */ }
    }
    setDepsMap(next);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tasks.length]);

  // F-03: blocked 派生（any 前置非终态——与内核派生态同语义;只在本组件展示层计算）
  const blockedIds = useMemo(() => {
    const s = new Set<string>();
    for (const [tid, pres] of Object.entries(depsMap)) {
      const anyOpen = pres.some((pid) => {
        const pt = tasks.find((t) => t.id === pid);
        return pt != null && pt.status !== 'done';
      });
      if (anyOpen) s.add(tid);
    }
    return s;
  }, [depsMap, tasks]);

  const addDep = (taskId: string, dependsOn: string) => {
    setDepErr(null);
    const out = platform.taskSetDependency(taskId, dependsOn);
    if (out !== 'ok') { setDepErr(out); return; }
    setDepsMap((m) => ({ ...m, [taskId]: [...new Set([...(m[taskId] ?? []), dependsOn])] }));
    setDepPick('');
  };

  const removeDep = (taskId: string, dependsOn: string) => {
    setDepErr(null);
    const out = platform.taskRemoveDependency(taskId, dependsOn);
    if (out !== 'ok') { setDepErr(out); return; }
    setDepsMap((m) => {
      const list = (m[taskId] ?? []).filter((d) => d !== dependsOn);
      const next = { ...m };
      if (list.length === 0) delete next[taskId]; else next[taskId] = list;
      return next;
    });
  };

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
            style={prioFilter === p.id ? { background: p.dot, color: 'var(--text-on-brand)', borderColor: p.dot } : undefined}
            onClick={() => setPrioFilter(prioFilter === p.id ? 'all' : p.id)}>
            {p.label} {base.filter((t) => (t.priority ?? 'normal') === p.id).length}
          </button>
        ))}
      </div>

      {shown.length === 0 ? (
        <EmptyState illustration="inbox" title="暂无任务。点右下角 + 捕获，加入今日任务。" />
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
                {list.map((t) => (
                  <TaskRowF04
                    key={t.id}
                    t={t}
                    tasks={tasks}
                    onTasks={onTasks}
                    showToast={showToast}
                    onOpenTask={onOpenTask}
                    lp={lp}
                    toggle={toggle}
                    cyclePrio={cyclePrio}
                    blockedIds={blockedIds}
                    depsMap={depsMap}
                    onManageDeps={(id) => { setDepSheet({ taskId: id }); setDepErr(null); setDepPick(''); }}
                    todayStr={todayStr}
                  />
                ))}
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

      {/* F-03: 依赖管理 sheet（前置任务区块——可搜添加/移除;环依赖可读提示） */}
      {depSheet && (() => {
        const t = tasks.find((x) => x.id === depSheet.taskId);
        if (!t) return null;
        const pres = (depsMap[t.id] ?? []).map((pid) => tasks.find((p) => p.id === pid)).filter(Boolean) as Task[];
        const candidates = open.filter((c) =>
          c.id !== t.id && !pres.some((p) => p.id === c.id) &&
          (depPick === '' || c.text.toLowerCase().includes(depPick.toLowerCase())));
        return (
          <>
            <div className="sheet-mask" onClick={() => setDepSheet(null)} />
            <div className="sheet" role="dialog" aria-label={`前置任务管理——${t.text}`}>
              <div className="sheet-handle" />
              <div className="sheet-title">前置任务<div className="sheet-sub">{t.text}</div></div>
              <div style={{ padding: '0 16px' }}>
                <input
                  className="search-input" style={{ width: '100%', marginBottom: 8 }}
                  placeholder="搜索任务设为前置…"
                  value={depPick}
                  onChange={(e) => setDepPick(e.target.value)}
                />
                {depErr && (
                  <div role="alert" style={{ padding: '8px 12px', marginBottom: 8, borderRadius: 8, background: 'var(--danger-soft-bg)', color: 'var(--danger)', fontSize: 13 }}>
                    {depErr}
                  </div>
                )}
                {pres.length === 0 && (
                  <div style={{ padding: '12px 0', opacity: 0.6, fontSize: 13 }}>无前置任务——从下方添加</div>
                )}
                {pres.map((p) => (
                  <div key={p.id} className="inbox-item" style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                    <span className={`task-check ${p.status === 'done' ? 'checked' : ''}`} style={{ width: 18, height: 18, flex: '0 0 auto' }}>{p.status === 'done' ? I.check : ''}</span>
                    <span style={{ flex: 1, fontSize: 13, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{p.text}</span>
                    <span style={{ fontSize: 11, opacity: 0.6 }}>{p.status === 'done' ? '已完成' : '未完成'}</span>
                    <button className="icon-btn" onClick={() => removeDep(t.id, p.id)} aria-label={`移除前置 ${p.text}`}>{I.x}</button>
                  </div>
                ))}
                {candidates.length > 0 && (
                  <div style={{ marginTop: 10 }}>
                    <div style={{ fontSize: 11, fontWeight: 600, opacity: 0.55, margin: '4px 0' }}>可添加</div>
                    {candidates.slice(0, 8).map((c) => (
                      <button key={c.id} type="button" className="inbox-item" style={{ display: 'flex', alignItems: 'center', gap: 8, width: '100%', cursor: 'pointer' }}
                        onClick={() => addDep(t.id, c.id)}>
                        <span style={{ fontSize: 13, flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', textAlign: 'left' }}>{c.text}</span>
                        <span className="task-dep-badge" style={{ fontSize: 11 }}>+ 前置</span>
                      </button>
                    ))}
                  </div>
                )}
              </div>
              <div className="sheet-actions">
                <button className="btn btn-text" onClick={() => setDepSheet(null)}>关闭</button>
              </div>
            </div>
          </>
        );
      })()}
    </div>
  );
}

/**
 * F-04+U-06: 任务行（useSwipeRow shared 单源——右滑完成/左滑明日;长按删+单击编辑保留）。
 * 行组件承载手势（Rules of Hooks 合规;闭包随渲染更新无旧值陷阱）。
 */
function TaskRowF04({ t, tasks, onTasks, showToast, onOpenTask, lp, toggle, cyclePrio, blockedIds, depsMap, onManageDeps, todayStr }: {
  t: Task;
  tasks: Task[];
  onTasks: (ts: Task[]) => void;
  showToast: (m: string) => void;
  onOpenTask?: (t: Task) => void;
  lp: React.MutableRefObject<{ timer: ReturnType<typeof setTimeout> | null; fired: boolean }>;
  toggle: (id: string) => void;
  cyclePrio: (id: string) => void;
  blockedIds: Set<string>;
  depsMap: Record<string, string[]>;
  onManageDeps: (id: string) => void;
  todayStr: string;
}) {
  const remove = () => {
    onTasks(tasks.filter((x) => x.id !== t.id));
    showToast('任务已删除');
  };
  const swipe = useSwipeRow({
    // F-04: 右滑=完成/恢复（误触可反向滑恢复——与 toggle 同链）
    onRight: () => {
      toggle(t.id);
      showToast(t.status === 'done' ? '已恢复未完成' : '已完成 ✓');
    },
    // F-04: 左滑=移入计划（明日）——快速整理链
    onLeft: () => {
      const tomorrow = new Date(Date.now() + 86400000).toISOString().slice(0, 10);
      onTasks(tasks.map((x) => x.id === t.id ? { ...x, due: tomorrow, status: 'plan' } : x));
      showToast('已移入计划（明日）');
    },
  });
  const subs = tasks.filter((st) => st.parentTaskId === t.id);
  const subDone = subs.filter((st) => st.status === 'done').length;
  return (
    <div className="task-item"
      onClick={() => { if (lp.current.fired) { lp.current.fired = false; return; } onOpenTask?.(t); }}
      onTouchStart={(e) => { lp.current.fired = false; lp.current.timer = setTimeout(() => { lp.current.fired = true; lp.current.timer = null; remove(); }, 550); swipe.onTouchStart(e); }}
      onTouchMove={(e) => { if (lp.current.timer) { clearTimeout(lp.current.timer); lp.current.timer = null; } swipe.onTouchMove(e); }}
      onTouchEnd={(e) => { if (lp.current.timer) { clearTimeout(lp.current.timer); lp.current.timer = null; } swipe.onTouchEnd(e); }}
    >
      <button
        className={`task-check ${t.status === 'done' ? 'checked' : ''}`}
        onClick={(e) => { e.stopPropagation(); toggle(t.id); }}
        aria-label={t.status === 'done' ? '恢复未完成' : '完成'}
      >{I.check}</button>
      <button className="task-prio-dot-btn" aria-label="优先级" title="切换优先级"
        onClick={(e) => { e.stopPropagation(); cyclePrio(t.id); }}>
        <span className="task-prio-dot" style={{ background: TASK_PRIO.find((p) => p.id === (t.priority ?? 'normal'))?.dot }} />
      </button>
      <div className="task-body">
        <div className="task-text">{t.text}</div>
        <div className="task-meta">
          <span className="task-status-tag">{TASK_GROUPS.find((g) => g.id === t.status)?.label}</span>
          {blockedIds.has(t.id) && (
            <span className="task-dep-badge" role="img" aria-label="被前置任务阻塞">⛔ 等待前置</span>
          )}
          {t.due && <span className={`task-due ${t.due < todayStr && t.status !== 'done' ? 'overdue' : ''}`}>{t.due}</span>}
          {subs.length > 0 && <span className="task-sub-prog">{subDone}/{subs.length}</span>}
          <span className="task-meta-time">{relativeTime(new Date(t.createdAt).toISOString())}</span>
        </div>
        {(depsMap[t.id]?.length ?? 0) > 0 && (
          <button type="button" className="task-dep-row"
            onClick={(e) => { e.stopPropagation(); onManageDeps(t.id); }}
            aria-label={`管理前置任务（${depsMap[t.id]!.length} 项）`}>
            前置 {depsMap[t.id]!.length} 项 ⏳
          </button>
        )}
      </div>
      <button className="icon-btn" onClick={(e) => { e.stopPropagation(); remove(); }} aria-label="删除任务">{I.x}</button>
    </div>
  );
}
