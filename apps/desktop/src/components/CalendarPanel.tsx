/**
 * A-08: 桌面日历视图——双端对称（同一 Rust CalendarVm,前端只渲染）。
 *
 * - 数据: cmd_calendar_vm() → CalendarVm { events, tasks }（core 权威源;
 *   重复展开=shared calendar.ts expandAll 单份实现——与移动同源）;
 * - 月视图: 6×7 网格（周一始,与移动周页对齐）——事件色点+任务数+农历副标;
 * - 周视图: 7 列日卡（事件+带 due 任务）——任务卡 HTML5 拖拽到别日 →
 *   cmd_reschedule_task（WritePath TaskRescheduled 事件——投影/双端同步）;
 * - 键盘: 网格 role=grid+方向键移动+Enter 进周视图+PgUp/PgDn 翻月;
 * - a11y: 格 aria-label 用 shared calendarCellAria（含农历/假日/计数——DoD）;
 * - 呈现纪律: 文本原样渲染;invoke=null（browser-mock）→ 空态明示,
 *   失败 → 错误条+重试（铁律 2: 禁静默假数据）。
 */
import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import tokens from '../design/tokens';
import {
  expandAll,
  instanceDayKey,
  lunarSub,
  calendarCellAria,
  type CalendarVm,
  type EvInstance,
} from '@aurora/shared-types';

type InvokeFn = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;

const pad = (n: number) => String(n).padStart(2, '0');
const dateKey = (d: Date) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
const EV_COLORS = ['#E0605A', '#5AA9E6', '#7BC96F', '#E6B35A', '#A78BFA', '#4EC9B0'];

export default function CalendarPanel(props: {
  invoke: InvokeFn | null;
  onOpenNote: (id: string) => void;
}): React.ReactElement {
  const { invoke, onOpenNote } = props;
  const [vm, setVm] = useState<CalendarVm | null>(null);
  const [failed, setFailed] = useState(false);
  const [viewMode, setViewMode] = useState<'month' | 'week'>('month');
  const [selected, setSelected] = useState(dateKey(new Date()));
  const [cursor, setCursor] = useState(() => {
    const n = new Date();
    return { y: n.getFullYear(), m: n.getMonth() };
  });
  const [dragOver, setDragOver] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const gridRef = useRef<HTMLDivElement>(null);

  /** VM 加载（改期后重拉——另一端同源投影随事件同步）。 */
  const reload = useCallback(
    (alive: { current: boolean }) => {
      if (!invoke) return;
      invoke('cmd_calendar_vm')
        .then((r) => {
          if (alive.current) {
            setVm((r as CalendarVm) ?? null);
            setFailed(false);
          }
        })
        .catch(() => {
          if (alive.current) setFailed(true);
        });
    },
    [invoke],
  );

  useEffect(() => {
    const alive = { current: true };
    reload(alive);
    return () => {
      alive.current = false;
    };
  }, [reload]);

  /** 月界 [月初-6周, 月末+1周] 展开实例 + due 任务分日（repeatUtil 单份）。 */
  const monthInstances = useMemo(() => {
    if (!vm) return [] as EvInstance[];
    const from = new Date(cursor.y, cursor.m - 1, 0);
    const to = new Date(cursor.y, cursor.m + 2, 0);
    return expandAll(vm.events, from, to);
  }, [vm, cursor]);

  const dayEvents = useCallback(
    (key: string) => monthInstances.filter((i) => instanceDayKey(i) === key),
    [monthInstances],
  );

  const dayTasks = useCallback(
    (key: string) => (vm ? vm.tasks.filter((t) => t.due === key) : []),
    [vm],
  );

  /** 月网格 6×7（周一始）。 */
  const cells = useMemo(() => {
    const first = new Date(cursor.y, cursor.m, 1);
    const lead = (first.getDay() + 6) % 7;
    const out: Date[] = [];
    for (let i = 0; i < 42; i++) out.push(new Date(cursor.y, cursor.m, 1 - lead + i));
    return out;
  }, [cursor]);

  /** 周视图 7 天（选中日所在周,周一始）。 */
  const weekDays = useMemo(() => {
    const [sy, sm, sd] = selected.split('-').map(Number);
    const d0 = new Date(sy, sm - 1, sd);
    const lead = (d0.getDay() + 6) % 7;
    return Array.from({ length: 7 }, (_, i) => {
      const d = new Date(sy, sm - 1, sd - lead + i);
      return d;
    });
  }, [selected]);

  /** 键盘导航: 方向键移 selected（跨月翻 cursor）;Enter 周视图;PgUp/Dn 翻月。 */
  const onGridKey = (e: React.KeyboardEvent) => {
    const [sy, sm, sd] = selected.split('-').map(Number);
    let d = new Date(sy, sm - 1, sd);
    const move = (days: number) => {
      d = new Date(sy, sm - 1, sd + days);
      setSelected(dateKey(d));
      setCursor({ y: d.getFullYear(), m: d.getMonth() });
    };
    if (e.key === 'ArrowLeft') { move(-1); e.preventDefault(); }
    else if (e.key === 'ArrowRight') { move(1); e.preventDefault(); }
    else if (e.key === 'ArrowUp') { move(-7); e.preventDefault(); }
    else if (e.key === 'ArrowDown') { move(7); e.preventDefault(); }
    else if (e.key === 'Enter') { setViewMode('week'); e.preventDefault(); }
    else if (e.key === 'PageUp') { setCursor((c) => (c.m === 0 ? { y: c.y - 1, m: 11 } : { y: c.y, m: c.m - 1 })); e.preventDefault(); }
    else if (e.key === 'PageDown') { setCursor((c) => (c.m === 11 ? { y: c.y + 1, m: 0 } : { y: c.y, m: c.m + 1 })); e.preventDefault(); }
  };

  /** 拖拽改期: 任务卡 drop 到日列 → cmd_reschedule_task（TaskRescheduled）。 */
  const onDropTask = async (e: React.DragEvent, dayKey: string) => {
    e.preventDefault();
    setDragOver(null);
    const taskId = e.dataTransfer.getData('text/task-id');
    if (!taskId || !invoke) return;
    setBusy(true);
    try {
      await invoke('cmd_reschedule_task', { taskId, newDue: dayKey });
      const alive = { current: true };
      reload(alive);
    } catch (err) {
      setFailed(true);
    } finally {
      setBusy(false);
    }
  };

  if (!invoke) {
    return (
      <p style={{ color: tokens.color.textSecondary, padding: tokens.spacing.lg }}>
        browser-mock 模式——日历仅 tauri 模式可用（A-08: cmd_calendar_vm 需 core）
      </p>
    );
  }
  if (failed) {
    return (
      <p style={{ color: tokens.color.textSecondary, padding: tokens.spacing.lg }}>
        日历数据加载失败——
        <button
          onClick={() => { setFailed(false); reload({ current: true }); }}
          style={{ color: tokens.color.primaryBright, background: 'none', border: 'none', cursor: 'pointer' }}
        >重试</button>
      </p>
    );
  }
  if (!vm) {
    return <p style={{ color: tokens.color.textSecondary, padding: tokens.spacing.lg }}>日历加载中…</p>;
  }

  const cellStyle = (isSel: boolean, isToday: boolean, inMonth: boolean): React.CSSProperties => ({
    minHeight: 72,
    display: 'flex',
    flexDirection: 'column',
    gap: 2,
    padding: 4,
    background: isSel ? 'rgba(88,166,255,0.10)' : 'transparent',
    border: `1px solid ${isSel || isToday ? tokens.color.primaryBright : 'rgba(255,255,255,0.10)'}`,
    borderRadius: tokens.radius.sm,
    opacity: inMonth ? 1 : 0.45,
    cursor: 'pointer',
    textAlign: 'left',
  });

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', overflow: 'hidden' }}>
      {/* 工具条: 月/周切换+翻月+农历月名 */}
      <div style={{ display: 'flex', alignItems: 'center', gap: tokens.spacing.sm, padding: tokens.spacing.sm, borderBottom: `1px solid ${'rgba(255,255,255,0.10)'}` }}>
        <button onClick={() => setViewMode(viewMode === 'month' ? 'week' : 'month')}
          style={{ ...btnBase, width: 'auto', padding: '4px 12px' }}>
          {viewMode === 'month' ? '周视图' : '月视图'}
        </button>
        <button aria-label="上一月" onClick={() => setCursor((c) => (c.m === 0 ? { y: c.y - 1, m: 11 } : { y: c.y, m: c.m - 1 }))} style={btnBase}>◀</button>
        <strong style={{ color: tokens.color.textPrimary, minWidth: 96, textAlign: 'center' }}>
          {cursor.y} 年 {cursor.m + 1} 月
        </strong>
        <button aria-label="下一月" onClick={() => setCursor((c) => (c.m === 11 ? { y: c.y + 1, m: 0 } : { y: c.y, m: c.m + 1 }))} style={btnBase}>▶</button>
        {busy && <span style={{ color: tokens.color.textSecondary, fontSize: 12 }}>同步中…</span>}
      </div>

      {viewMode === 'month' ? (
        <div role="grid" aria-label={`${cursor.y} 年 ${cursor.m + 1} 月日历`} tabIndex={0} ref={gridRef}
          onKeyDown={onGridKey}
          style={{ display: 'grid', gridTemplateColumns: 'repeat(7, 1fr)', gap: 4, padding: tokens.spacing.sm, overflow: 'auto', minHeight: 0, flex: 1 }}>
          {['一', '二', '三', '四', '五', '六', '日'].map((w) => (
            <div key={w} role="columnheader" style={{ color: tokens.color.textSecondary, fontSize: 11, textAlign: 'center', padding: '2px 0' }}>{w}</div>
          ))}
          {cells.map((d) => {
            const key = dateKey(d);
            const evs = dayEvents(key);
            const tks = dayTasks(key);
            const isSel = key === selected;
            const isToday = key === dateKey(new Date());
            const inMonth = d.getMonth() === cursor.m;
            const ls = lunarSub(d.getDate(), d.getFullYear(), d.getMonth());
            return (
              <button
                key={key}
                role="gridcell"
                aria-label={calendarCellAria(d.getDate(), d.getFullYear(), d.getMonth(), evs.length, tks.length)}
                aria-selected={isSel}
                onClick={() => setSelected(key)}
                onDoubleClick={() => { setSelected(key); setViewMode('week'); }}
                style={cellStyle(isSel, isToday, inMonth)}
              >
                <span style={{ display: 'flex', alignItems: 'baseline', gap: 4 }}>
                  <strong style={{ color: isToday ? tokens.color.primaryBright : tokens.color.textPrimary, fontSize: 13 }}>{d.getDate()}</strong>
                  <span style={{ color: ls.isStat || ls.isWork ? tokens.color.primaryBright : tokens.color.textSecondary, fontSize: 10 }}>{ls.sub}</span>
                </span>
                {evs.length > 0 && (
                  <span style={{ display: 'flex', gap: 3, alignItems: 'center' }}>
                    {[...new Set(evs.map((i) => i.ev.color % 6))].slice(0, 4).map((c) => (
                      <i key={c} aria-hidden style={{ width: 6, height: 6, borderRadius: '50%', background: EV_COLORS[c], display: 'inline-block' }} />
                    ))}
                  </span>
                )}
                {tks.length > 0 && (
                  <span style={{ fontSize: 10, color: tokens.color.textSecondary }}>✓{tks.length} 待办</span>
                )}
              </button>
            );
          })}
        </div>
      ) : (
        /* 周视图: 7 列日卡——事件列表+任务卡（可拖拽改期）。 */
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(7, 1fr)', gap: 4, padding: tokens.spacing.sm, overflow: 'auto', minHeight: 0, flex: 1 }}
          role="grid" aria-label={`周视图 ${weekDays[0] ? dateKey(weekDays[0]) : ''} 起`}>
          {weekDays.map((d) => {
            const key = dateKey(d);
            const evs = dayEvents(key);
            const tks = dayTasks(key);
            const isSel = key === selected;
            const isToday = key === dateKey(new Date());
            const ls = lunarSub(d.getDate(), d.getFullYear(), d.getMonth());
            return (
              <div
                key={key}
                role="gridcell"
                aria-label={calendarCellAria(d.getDate(), d.getFullYear(), d.getMonth(), evs.length, tks.length)}
                onDragOver={(e) => { e.preventDefault(); setDragOver(key); }}
                onDragLeave={() => setDragOver((k) => (k === key ? null : k))}
                onDrop={(e) => onDropTask(e, key)}
                style={{
                  display: 'flex', flexDirection: 'column', gap: 4,
                  background: dragOver === key ? 'rgba(88,166,255,0.12)' : 'transparent',
                  border: `1px solid ${isSel ? tokens.color.primaryBright : 'rgba(255,255,255,0.10)'}`,
                  borderRadius: tokens.radius.md,
                  padding: tokens.spacing.sm,
                  minHeight: 0,
                }}
              >
                <button onClick={() => setSelected(key)} style={{ background: 'none', border: 'none', cursor: 'pointer', padding: 0, textAlign: 'left' }}>
                  <strong style={{ color: isToday ? tokens.color.primaryBright : tokens.color.textPrimary, fontSize: 13 }}>
                    {d.getMonth() + 1}/{d.getDate()}
                  </strong>
                  <span style={{ color: tokens.color.textSecondary, fontSize: 10, marginLeft: 4 }}>{ls.sub}</span>
                </button>
                {evs.map((i) => (
                  <button key={`${i.ev.id}@${i.offsetDays}`}
                    onClick={() => { if (i.ev.note_id) onOpenNote(i.ev.note_id); }}
                    title={i.ev.title}
                    style={{ fontSize: 11, textAlign: 'left', padding: '2px 6px', borderRadius: tokens.radius.sm, border: 'none', cursor: i.ev.note_id ? 'pointer' : 'default', background: 'transparent', color: EV_COLORS[i.ev.color % 6] }}>
                    {i.ev.all_day ? '☰ ' : ''}
                    {i.ev.title}
                  </button>
                ))}
                {tks.map((t) => (
                  <div
                    key={t.id}
                    draggable
                    onDragStart={(e) => e.dataTransfer.setData('text/task-id', t.id)}
                    title="拖拽到其他日期列改期（TaskRescheduled）"
                    style={{
                      fontSize: 11, padding: '3px 6px', borderRadius: tokens.radius.sm,
                      background: t.status === 'done' ? 'transparent' : tokens.color.bgElevated,
                      color: t.status === 'done' ? tokens.color.textSecondary : tokens.color.textPrimary,
                      textDecoration: t.status === 'done' ? 'line-through' : 'none',
                      cursor: 'grab',
                    }}
                  >
                    {t.status === 'done' ? '✓ ' : '• '}{t.text}
                  </div>
                ))}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

const btnBase: React.CSSProperties = {
  width: 28, height: 28, display: 'flex', alignItems: 'center', justifyContent: 'center',
  background: tokens.color.bgSurface, color: tokens.color.textPrimary,
  border: `1px solid ${'rgba(255,255,255,0.10)'}`, borderRadius: tokens.radius.sm, cursor: 'pointer',
  fontSize: 12,
};
