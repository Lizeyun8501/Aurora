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
  /* C-10: 周视图双布局——cards（7 列日卡,既有）/axis（24h 像素轴,
   * A-08 登记的移动端等价形态）。默认 axis（视觉时刻感知）,可切回。 */
  const [weekLayout, setWeekLayout] = useState<'cards' | 'axis'>('axis');
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

  /** C-10: 事件时刻 span（分钟域——axis 定位用）。
   * start 必有;end 缺=60min 默认;跨日 clamp 至当日 [0,1440]。
   * all_day 事件不参与轴定位（顶部条呈现）。 */
  const evSpan = useCallback((ev: { start: string; end: string | null; all_day: boolean }): { top: number; dur: number } | null => {
    if (ev.all_day) return null;
    const s0 = new Date(ev.start);
    if (Number.isNaN(s0.getTime())) return null;
    const dayStart = new Date(s0.getFullYear(), s0.getMonth(), s0.getDate());
    let top = (s0.getTime() - dayStart.getTime()) / 60000;
    const e0 = ev.end ? new Date(ev.end) : null;
    let dur = e0 && !Number.isNaN(e0.getTime()) ? (e0.getTime() - s0.getTime()) / 60000 : 60;
    if (top < 0) { dur += top; top = 0; }          // 跨日起始（前日深夜续入）
    if (top + dur > 1440) dur = 1440 - top;         // 跨日溢出 clamp
    if (dur < 20) dur = 20;                         // 最小可读高（半行）
    return { top: Math.max(0, top), dur: Math.min(dur, 1440) };
  }, []);

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

  /* C-10: axis 滚动锚定——今日红线居中/他日 08:00 惯例（移动周页同款）。 */
  const axisBodyRef = useRef<HTMLDivElement | null>(null);
  useEffect(() => {
    if (viewMode !== 'week' || weekLayout !== 'axis') return;
    const el = axisBodyRef.current;
    if (!el) return;
    const now = new Date();
    const isThisWeek = weekDays.some((d) => dateKey(d) === dateKey(now));
    const anchorMin = isThisWeek ? now.getHours() * 60 + now.getMinutes() : 8 * 60;
    const rowH = AXIS_ROW_H;
    const target = (anchorMin / 60) * rowH - el.clientHeight / 2 + rowH / 2;
    el.scrollTo({ top: Math.max(0, target) });
  }, [viewMode, weekLayout, weekDays]);

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
        {viewMode === 'week' && (
          <button
            aria-label={weekLayout === 'axis' ? '切换为列卡布局' : '切换为 24 小时轴布局'}
            onClick={() => setWeekLayout(weekLayout === 'axis' ? 'cards' : 'axis')}
            style={{ ...btnBase, width: 'auto', padding: '4px 12px', fontSize: 11 }}
            title="C-10: 周视图双布局——24h 轴（时刻定位）/列卡（密度紧凑）"
          >
            {weekLayout === 'axis' ? '☰ 列卡' : '⏱ 24h 轴'}
          </button>
        )}
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
      ) : weekLayout === 'cards' ? (
        /* 周视图（列卡布局）: 7 列日卡——事件列表+任务卡（可拖拽改期）。 */
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
      ) : (
        /* C-10 周视图（24h 轴布局）: 时刻列+7 日列——事件块分钟域定位
         * （all_day 顶部条;跨日 clamp;最小 20px 可读高）;今日红线;
         * 任务袋列底（无时刻诚实呈现,拖拽改期通路保留）;a11y 行
         * aria-label（时刻+计数）;role=gridcell 列头可拖放。 */
        <div style={{ display: 'flex', flexDirection: 'column', minHeight: 0, flex: 1, overflow: 'hidden' }}>
          {/* all_day 事件带（跨列） */}
          {(() => {
            const allDay = weekDays.flatMap((d) => dayEvents(dateKey(d)).filter((i) => i.ev.all_day));
            const uniq = Array.from(new Map(allDay.map((i) => [`${instanceKeyC10(i)}`, i])).values());
            if (uniq.length === 0) return null;
            return (
              <div role="row" aria-label={`全天事件 ${uniq.length} 项`} style={{ display: 'flex', flexWrap: 'wrap', gap: 4, padding: '4px 8px', borderBottom: '1px solid rgba(255,255,255,0.10)' }}>
                {uniq.map((i) => (
                  <span key={instanceKeyC10(i)} style={{ fontSize: 11, padding: '2px 8px', borderRadius: tokens.radius.sm, background: 'rgba(255,255,255,0.06)', color: tokens.color.textPrimary, border: `1px solid ${EV_COLORS[i.ev.color % 6]}` }}>
                    🌐 {i.ev.title}
                  </span>
                ))}
              </div>
            );
          })()}
          <div style={{ display: 'grid', gridTemplateColumns: '48px repeat(7, 1fr)', gap: 0, flex: 1, minHeight: 0, overflow: 'hidden' }}>
            {/* 表头行: 时刻列空位+7 日头（可拖放改期锚点+选中/今日态） */}
            <div role="columnheader" aria-label="时刻" style={{ fontSize: 10, color: tokens.color.textSecondary, textAlign: 'center', padding: '2px 0', borderBottom: '1px solid rgba(255,255,255,0.10)' }}>时</div>
            {weekDays.map((d) => {
              const key = dateKey(d);
              const isSel = key === selected;
              const isToday = key === dateKey(new Date());
              return (
                <button
                  key={key}
                  role="columnheader"
                  aria-label={`周视图列 ${key}`}
                  onClick={() => setSelected(key)}
                  onDragOver={(e) => { e.preventDefault(); setDragOver(key); }}
                  onDragLeave={() => setDragOver(null)}
                  onDrop={(e) => void onDropTask(e, key)}
                  style={{
                    fontSize: 11, padding: '3px 0', cursor: 'pointer', textAlign: 'center',
                    background: dragOver === key ? 'rgba(88,166,255,0.18)' : 'transparent',
                    color: isToday ? tokens.color.primaryBright : isSel ? tokens.color.textPrimary : tokens.color.textSecondary,
                    border: `1px solid ${isSel ? tokens.color.primaryBright : 'transparent'}`,
                    borderBottom: '1px solid rgba(255,255,255,0.10)',
                    fontWeight: isToday || isSel ? 700 : 400,
                  }}
                >
                  {`${d.getMonth() + 1}/${d.getDate()}`}周{'一二三四五六日'[(d.getDay() + 6) % 7]}
                </button>
              );
            })}
          </div>
          {/* 主体滚动区: 时刻列+7 日列（24 行×44px;事件块绝对定位）——
           * axisBodyRef 锚定今日红线/08:00 惯例（effect scrollTo）。
           * role=grid+onGridKey 键盘导航承继（方向/PgUp/Dn 同月视图）。 */}
          <div ref={axisBodyRef} role="grid" aria-label={`周视图 24 小时轴 ${weekDays[0] ? dateKey(weekDays[0]) : ''} 起`} tabIndex={0} onKeyDown={onGridKey}
            style={{ display: 'grid', gridTemplateColumns: '48px repeat(7, 1fr)', gap: 0, flex: 1, minHeight: 0, overflow: 'auto' }}>
            <div style={{ position: 'relative', borderRight: '1px solid rgba(255,255,255,0.06)' }}>
              {Array.from({ length: 24 }, (_, h) => (
                <div key={h} role="row" aria-label={`${pad(h)} 时`} style={{ height: `${AXIS_ROW_H}px`, fontSize: 10, color: tokens.color.textSecondary, textAlign: 'right', paddingRight: 6, paddingTop: 1, borderBottom: '1px dashed rgba(255,255,255,0.04)', boxSizing: 'border-box' }}>
                  {pad(h)}:00
                </div>
              ))}
            </div>
            {weekDays.map((d) => {
              const key = dateKey(d);
              const evs = dayEvents(key);
              const timed = evs.filter((i) => !i.ev.all_day);
              const tks = dayTasks(key);
              const isToday = key === dateKey(new Date());
              return (
                <div
                  key={key}
                  role="gridcell"
                  aria-label={calendarCellAria(d.getDate(), d.getFullYear(), d.getMonth(), evs.length, tks.length)}
                  onDragOver={(e) => { e.preventDefault(); setDragOver(key); }}
                  onDragLeave={() => setDragOver(null)}
                  onDrop={(e) => void onDropTask(e, key)}
                  style={{
                    position: 'relative',
                    height: `${AXIS_ROW_H * 24}px`,
                    background: dragOver === key ? 'rgba(88,166,255,0.10)' : 'transparent',
                    borderLeft: '1px solid rgba(255,255,255,0.04)',
                  }}
                >
                  {/* 整点刻度线 */}
                  {Array.from({ length: 24 }, (_, h) => (
                    <div key={h} style={{ position: 'absolute', top: `${h * AXIS_ROW_H}px`, left: 0, right: 0, height: 1, background: h % 6 === 0 ? 'rgba(255,255,255,0.08)' : 'rgba(255,255,255,0.04)' }} />
                  ))}
                  {/* 事件块（分钟域定位——同列交叠侧移防遮） */}
                  {timed.map((i, idx) => {
                    const span = evSpan(i.ev);
                    if (!span) return null;
                    return (
                      <div
                        key={instanceKeyC10(i)}
                        title={`${i.ev.title}${i.ev.end ? ` ${i.ev.start.slice(11, 16)}–${i.ev.end.slice(11, 16)}` : ''}`}
                        style={{
                          position: 'absolute',
                          top: `${(span.top / 60) * AXIS_ROW_H}px`,
                          height: `${Math.max(18, (span.dur / 60) * AXIS_ROW_H)}px`,
                          left: `${2 + (idx % 3) * 3}%`,
                          width: `${92 - (idx % 3) * 3}%`,
                          fontSize: 11,
                          lineHeight: 1.2,
                          padding: '1px 4px',
                          borderRadius: tokens.radius.sm,
                          overflow: 'hidden',
                          background: 'rgba(22,34,53,0.92)',
                          borderLeft: `3px solid ${EV_COLORS[i.ev.color % 6]}`,
                          color: tokens.color.textPrimary,
                          cursor: 'default',
                        }}
                      >
                        {i.ev.title}
                      </div>
                    );
                  })}
                  {/* 今日红线（现在时刻——渲染时求值,分钟粒度） */}
                  {isToday && (() => {
                    const now = new Date();
                    const nowMin = now.getHours() * 60 + now.getMinutes();
                    if (nowMin >= 1440) return null;
                    return (
                      <div
                        role="presentation"
                        aria-label={`现在 ${pad(now.getHours())}:${pad(now.getMinutes())}`}
                        style={{ position: 'absolute', top: `${(nowMin / 60) * AXIS_ROW_H}px`, left: 0, right: 0, height: 2, background: tokens.color.danger, boxShadow: '0 0 4px rgba(232,93,93,0.8)', zIndex: 2 }}
                      />
                    );
                  })()}
                  {/* 任务袋（列底悬浮——无时刻诚实呈现;拖拽源保留） */}
                  {tks.length > 0 && (
                    <div style={{ position: 'sticky', bottom: 0, marginTop: `${AXIS_ROW_H * 24 - 34}px`, background: 'rgba(16,21,28,0.92)', borderTop: '1px solid rgba(255,255,255,0.10)', padding: '2px 4px', zIndex: 3 }}>
                      {tks.slice(0, 3).map((t) => (
                        <div
                          key={t.id}
                          draggable
                          onDragStart={(e) => e.dataTransfer.setData('text/task-id', t.id)}
                          title="拖拽到其他日期列改期（TaskRescheduled）"
                          style={{ fontSize: 10, color: t.status === 'done' ? tokens.color.textSecondary : tokens.color.textPrimary, textDecoration: t.status === 'done' ? 'line-through' : 'none', cursor: 'grab', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}
                        >
                          {t.status === 'done' ? '✓ ' : '• '}{t.text}
                        </div>
                      ))}
                      {tks.length > 3 && <span style={{ fontSize: 10, color: tokens.color.textSecondary }}>+{tks.length - 3} 项</span>}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        </div>
      )}
    </div>
  );
}

/** C-10: 24h 轴行高（44px=tokens.a11y.minTouchTarget——时刻行兼触摸目标）。 */
const AXIS_ROW_H = 44;
/** C-10: 实例去重键（base@偏移——shared instanceKey 形,桌面本地小辅）。 */
const instanceKeyC10 = (i: EvInstance) => `${i.ev.id}@${i.offsetDays}`;

const btnBase: React.CSSProperties = {
  width: 28, height: 28, display: 'flex', alignItems: 'center', justifyContent: 'center',
  background: tokens.color.bgSurface, color: tokens.color.textPrimary,
  border: `1px solid ${'rgba(255,255,255,0.10)'}`, borderRadius: tokens.radius.sm, cursor: 'pointer',
  fontSize: 12,
};
