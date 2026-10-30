/**
 * CalendarView — 底部 Tab「日历」（月历 + 当日任务/笔记聚合）
 *
 * DK-CALENDAR-01 任务 4：从 MobileApp.tsx 拆出（2000+ 行瘦身）。
 * 纯搬移零逻辑改动（独立 commit）。
 *
 * 依赖说明：NoteCard/EmptyState/I/Task/TASK_GROUPS/listNotesTitled 仍在
 * MobileApp.tsx——本模块从其 import。ESM 函数级引用（组件体内使用，
 * 无模块顶层执行依赖）——环安全（MobileApp ↔ views 双向 import 的
 * React 组件环是既有生态常见形态）。
 */
import { useState } from 'react';
import { Solar, HolidayUtil, Lunar } from 'lunar-typescript';
import {
  type Task, TASK_GROUPS, I, listNotesTitled, NoteCard, EmptyState,
} from '../MobileApp';
import type { CalEvent, ReminderSetting } from '../adapters/androidPlatform';
import { expandAll, type EvInstance } from './repeatUtil';


export default function CalendarView({ tasks, events, onUpsertEvent, onDeleteEvent, onOpenNote, onNewTask, initialSelected }: {
  tasks: Task[];
  /** DK-CALENDAR-02：日程事件（MobileApp 层注入）。 */
  events: CalEvent[];
  onUpsertEvent: (ev: CalEvent) => void;
  onDeleteEvent: (id: string) => void;
  onOpenNote: (id: string, title: string) => void;
  onNewTask: (due: string) => void;
  /** 2026-10-28 周条联动预选日期——跨月自动翻页（如 11-01 → 月历翻到 11 月） */
  initialSelected?: string;
}) {
  const now = new Date();
  const todayStr = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}-${String(now.getDate()).padStart(2, '0')}`;
  /** 周条跳转的跨月偏移（目标月−当前月；仅挂载时消费一次） */
  const [monthOffset, setMonthOffset] = useState(() => {
    if (!initialSelected) return 0;
    const [yy, mm] = initialSelected.split('-').map(Number);
    return (yy - now.getFullYear()) * 12 + (mm - 1 - now.getMonth());
  });
  const [selected, setSelected] = useState<string>(initialSelected ?? todayStr);
  /** DK-CALENDAR-01 任务 2 + 04 四态：月/周/日/列表（年视图独立 mode 覆盖月）。 */
  const [mode, setMode] = useState<'month' | 'week' | 'day' | 'list' | 'year'>('month');
  /** DK-CALENDAR-02：事件编辑器（null=关;编辑/新建共用 sheet）。 */
  const [eventEditor, setEventEditor] = useState<CalEvent | null>(null);
  /** DK-CALENDAR-04 任务 3: 列表页筛选+搜索。 */
  const [listFilter, setListFilter] = useState<'all' | 'linked' | 'birthday'>('all');
  const [listQuery, setListQuery] = useState('');

  const view = new Date(now.getFullYear(), now.getMonth() + monthOffset, 1);
  const y = view.getFullYear();
  const m = view.getMonth();
  const lead = (new Date(y, m, 1).getDay() + 6) % 7; // 周一起始
  const daysInMonth = new Date(y, m + 1, 0).getDate();
  const fmt = (d: number) => `${y}-${String(m + 1).padStart(2, '0')}-${String(d).padStart(2, '0')}`;

  const notes = listNotesTitled();
  /** 当日数据: 笔记（updatedAt 日期匹配）+ 任务（due 匹配）+ 日记（title 含日期） */
  const dayData = (d: number) => {
    const key = fmt(d);
    // 对向复核修复（Leo 10-29）: updatedAt 为 RFC3339 UTC——startsWith 匹配公历日期会把
    // 本地 00:00-07:59 的笔记（UTC=前一天）错落到昨日格子。改本地日界比对。
    const dayNotes = notes.filter((n) => {
      const dt = new Date(n.updatedAt);
      if (Number.isNaN(dt.getTime())) return n.updatedAt.startsWith(key);
      return `${dt.getFullYear()}-${String(dt.getMonth() + 1).padStart(2, '0')}-${String(dt.getDate()).padStart(2, '0')}` === key;
    });
    const dayDiary = notes.filter((n) => n.title.includes(key) && n.title.includes('日记'));
    const dayTasks = tasks.filter((t) => t.due === key);
    return { dayNotes, dayDiary, dayTasks };
  };
  const hasMark = (d: number) => {
    const { dayNotes, dayDiary, dayTasks } = dayData(d);
    if (dayNotes.length > 0 || dayDiary.length > 0 || dayTasks.length > 0) return true;
    // DK-CALENDAR-03：月历日格事件角标（当月展开实例命中——重复事件已展开）
    const key = `${y}-${String(m + 1).padStart(2, '0')}-${String(d).padStart(2, '0')}`;
    return monthInstances.some((i) => eventLocalKey(i.start.toISOString()) === key);
  };

  /** DK-CALENDAR-02 任务 3 / 03 展开面：日格事件色点列表（最多 3 色）。 */
  const dayEventDots = (d: number): number[] => {
    const key = `${y}-${String(m + 1).padStart(2, '0')}-${String(d).padStart(2, '0')}`;
    const hits = monthInstances.filter((i) => eventLocalKey(i.start.toISOString()) === key);
    return [...new Set(hits.map((i) => i.ev.color % 6))].slice(0, 3);
  };

  // 农历/节假日 — lunar-typescript（6tail, 含法定节假日+调休数据随库更新）
  /** 格子副标: 法定假日名 > 农历节日 > 公历节日 > 节气 > 初一显月名 > 农历日。 */
  const lunarSub = (d: number): { sub: string; isStat: boolean; isWork: boolean } => {
    const solar = Solar.fromYmd(y, m + 1, d);
    const lunar = solar.getLunar();
    const h = HolidayUtil.getHoliday(y, m + 1, d);
    let sub: string;
    if (h) sub = h.getName();
    else if (lunar.getFestivals().length) sub = lunar.getFestivals()[0];
    else if (solar.getFestivals().length) sub = solar.getFestivals()[0];
    else if (lunar.getJieQi()) sub = lunar.getJieQi();
    else if (lunar.getDayInChinese() === '初一') sub = `农历${lunar.getMonthInChinese()}月`;
    else sub = lunar.getDayInChinese();
    return { sub, isStat: !!h && !h.isWork(), isWork: !!h && h.isWork() };
  };
  /** 选中日农历全称（如 二〇二六年正月初一）+ 假日/节气标注。 */
  const selLunarInfo = (() => {
    const [sy, sm, sd] = selected.split('-').map(Number);
    const solar = Solar.fromYmd(sy, sm, sd);
    const lunar = solar.getLunar();
    const tags = [
      ...lunar.getFestivals(),
      ...solar.getFestivals(),
      lunar.getJieQi(),
    ].filter(Boolean);
    const h = HolidayUtil.getHoliday(sy, sm, sd);
    const rest = h ? (h.isWork() ? '（调休上班）' : '（法定假日）') : '';
    /** DK-CALENDAR-01 黄历宜忌（各取前 4 项，lunar-typescript 数据现成） */
    const yi = lunar.getDayYi().slice(0, 4);
    const ji = lunar.getDayJi().slice(0, 4);
    return { text: lunar.toString(), tag: tags.length ? tags[0] + rest : rest, yi, ji };
  })();

  const selD = parseInt(selected.slice(8), 10);
  const selData = dayData(selD);
  const isSelInMonth = selected.startsWith(`${y}-${String(m + 1).padStart(2, '0')}`);

  /** 选中日事件实例（DK-CALENDAR-03: 重复事件展开 + 02 跨天单次保留）。
   * 时区纪律：所有日界/小时换算一律 Date 本地解析——Leo 8f82772 教训。 */
  const eventLocalKey = (iso: string): string => {
    const d = new Date(iso);
    if (Number.isNaN(d.getTime())) return '';
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
  };

  /** DK-CALENDAR-03: 当月展开实例（角标消费——月窗 [1 号, 月末]）。 */
  const monthInstances: EvInstance[] = (() => {
    const from = new Date(y, m, 1);
    const to = new Date(y, m + 1, 0, 23, 59, 59);
    return expandAll(events, from, to);
  })();

  /** 选中日实例集（重复展开 + 跨天单次 02 语义保留;id@offset 去重）。 */
  const selEvents: EvInstance[] = (() => {
    const from = new Date(`${selected}T00:00:00`);
    const to = new Date(`${selected}T23:59:59`);
    const out = expandAll(events, from, to);
    // 02 跨天单次（sKey≤selected≤eKey——start 不在窗但交叠）
    for (const e of events) {
      if (e.repeat) continue;
      const s = new Date(e.start);
      if (Number.isNaN(s.getTime())) continue;
      const sKey = eventLocalKey(e.start);
      const eKey = e.end ? eventLocalKey(e.end) : sKey;
      if (selected >= sKey && selected <= eKey && !out.some((i) => i.ev.id === e.id)) {
        out.push({ ev: e, start: s, offsetDays: 0 });
      }
    }
    return out;
  })();

  /** 事件色板 0-5（V25 语义色——红/蓝/绿/琥珀/紫/灰）。 */
  const EV_COLORS = ['#dc2626', '#2563eb', '#059669', '#d97706', '#7c3aed', '#6b7280'];

  /** DK-CALENDAR-01 任务 2：日视图时间轴聚合。
   * 任务（due 无时刻）→ 全天区；日记/笔记按 updated_at 本地小时落轴
   * （P0 展示聚合——NoteSummary 无 created_at，事件实体真时间 02 卡引入）。
   * 时间轴 08-22，轴外聚合入「更早/更晚」兜底区。 */
  const dayAxis = (() => {
    const hour = (iso: string): number => {
      const d = new Date(iso);
      return Number.isNaN(d.getTime()) ? -1 : d.getHours();
    };
    type AxisItem = { kind: 'task' | 'note' | 'diary' | 'event'; label: string; id: string; ev?: CalEvent; instStart?: Date };
    const slots = new Map<number, AxisItem[]>();
    const push = (h: number, item: AxisItem) => {
      const arr = slots.get(h) ?? [];
      arr.push(item);
      slots.set(h, arr);
    };
    for (const t of selData.dayTasks) push(-1, { kind: 'task', label: t.text, id: t.id });
    for (const n of selData.dayDiary) push(hour(n.updatedAt), { kind: 'diary', label: n.title, id: n.id });
    for (const n of selData.dayNotes) push(hour(n.updatedAt), { kind: 'note', label: n.title, id: n.id });
    // DK-CALENDAR-02 任务 4 聚合混排 / 03 实例面: 展开实例色块
    // （all_day→全天区;时刻实例按 inst.start 本地小时——实例时刻而非锚点）
    for (const inst of selEvents) {
      const h = inst.ev.all_day ? -1 : inst.start.getHours();
      push(h, { kind: 'event', label: inst.ev.title, id: inst.ev.id, ev: inst.ev, instStart: inst.start });
    }
    return slots;
  })();

  return (
    <div className="view">
      <div className="card">
        <div className="cal-head">
          <button className="icon-btn" onClick={() => setMonthOffset((o) => o - 1)} aria-label="上月">{I.back}</button>
          <span className="cal-title" role="button" aria-label="查看年视图"
            onClick={() => setMode('year')}>{y} 年 {m + 1} 月</span>
          <button className="icon-btn" onClick={() => setMonthOffset((o) => o + 1)} aria-label="下月">
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round"><path d="m9 18 6-6-6-6"/></svg>
          </button>
          <div className="cal-mode-seg" role="tablist" aria-label="日历视图切换">
            <button className={`nt-seg ${mode === 'month' ? 'on' : ''}`} role="tab" aria-selected={mode === 'month'} onClick={() => setMode('month')}>月</button>
            <button className={`nt-seg ${mode === 'week' ? 'on' : ''}`} role="tab" aria-selected={mode === 'week'} onClick={() => setMode('week')}>周</button>
            <button className={`nt-seg ${mode === 'day' ? 'on' : ''}`} role="tab" aria-selected={mode === 'day'} onClick={() => setMode('day')}>日</button>
            <button className={`nt-seg ${mode === 'list' ? 'on' : ''}`} role="tab" aria-selected={mode === 'list'} onClick={() => setMode('list')}>列表</button>
          </div>
        </div>
        {mode === 'month' && (
          <div className="cal-grid" key={monthOffset}>
            {['一', '二', '三', '四', '五', '六', '日'].map((w) => (
              <span key={w} className="cal-week">{w}</span>
            ))}
            {Array.from({ length: lead }, (_, i) => <span key={`e${i}`} />)}
            {Array.from({ length: daysInMonth }, (_, i) => {
              const d = i + 1;
              const key = fmt(d);
              const isToday = key === todayStr;
              const isSel = key === selected;
              const ls = lunarSub(d);
              return (
                <button
                  key={d}
                  className={`cal-day cal-day-btn ${isToday ? 'today' : ''} ${isSel ? 'sel' : ''} ${hasMark(d) ? 'has' : ''} ${ls.isStat ? 'fest' : ''} ${ls.isWork ? 'workday' : ''}`}
                  onClick={() => setSelected(key)}
                >
                  <span className="cal-num">{d}</span>
                  {!isToday && <span className={`cal-lunar ${(ls.isStat || ls.sub.length > 2) && !ls.isWork ? 'fest' : ''}`}>{ls.isWork ? '班' : ls.sub}</span>}
                  {/* DK-CALENDAR-02 任务 3：事件色点角标（has 点位被任务/笔记占——事件点独立行显色） */}
                  {dayEventDots(d).length > 0 && (
                    <span className="cal-ev-dots">
                      {dayEventDots(d).map((c) => (
                        <i key={c} className="cal-ev-dot" style={{ background: EV_COLORS[c] }} />
                      ))}
                    </span>
                  )}
                </button>
              );
            })}
          </div>
        )}
        {mode === 'week' && (() => {
          /** DK-CALENDAR-04 任务 1: 周视图——7 天并排+当日实例色块列。
           * 周界=selected 所在周一;列头点入切日视图。 */
          const wStart = new Date(`${selected}T00:00:00`);
          wStart.setDate(wStart.getDate() - ((wStart.getDay() + 6) % 7));
          const days = Array.from({ length: 7 }, (_, i) => {
            const d = new Date(wStart);
            d.setDate(d.getDate() + i);
            return d;
          });
          const WD = ['一', '二', '三', '四', '五', '六', '日'];
          const pad = (n: number) => String(n).padStart(2, '0');
          const dKey = (d: Date) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
          return (
            <div className="week-grid" key={dKey(wStart)}>
              {days.map((d, i) => {
                const k = dKey(d);
                const insts = expandAll(events, new Date(`${k}T00:00:00`), new Date(`${k}T23:59:59`));
                const isToday = k === todayStr;
                return (
                  <div key={k} className={`week-col ${isToday ? 'today' : ''}`}>
                    <button className="week-col-head" aria-label={`查看 ${k}`}
                      onClick={() => { setSelected(k); setMode('day'); }}>
                      <span className="week-wd">{WD[i]}</span>
                      <span className="week-num">{d.getDate()}</span>
                    </button>
                    <div className="week-col-items">
                      {insts.slice(0, 5).map((inst) => (
                        <span key={`${inst.ev.id}@${inst.offsetDays}`} className="day-axis-item event"
                          role="button" style={{
                            background: `rgba(${parseInt(EV_COLORS[inst.ev.color % 6].slice(1, 3), 16)}, ${parseInt(EV_COLORS[inst.ev.color % 6].slice(3, 5), 16)}, ${parseInt(EV_COLORS[inst.ev.color % 6].slice(5, 7), 16)}, 0.15)`,
                            color: EV_COLORS[inst.ev.color % 6],
                          }} onClick={() => setEventEditor(inst.ev)}>{inst.ev.title}</span>
                      ))}
                      {insts.length > 5 && <span className="week-more">+{insts.length - 5} 更多</span>}
                    </div>
                  </div>
                );
              })}
            </div>
          );
        })()}
        {mode === 'year' && (() => {
          /** DK-CALENDAR-04 任务 2: 年视图——12 月缩略格（月名+密度微标）。
           * 点月格→monthOffset 定位+切月视图。 */
          const y2 = view.getFullYear();
          return (
            <div className="year-grid" key={y2}>
              {Array.from({ length: 12 }, (_, i) => {
                const mFrom = new Date(y2, i, 1);
                const mTo = new Date(y2, i + 1, 0, 23, 59, 59);
                const cnt = expandAll(events, mFrom, mTo).length;
                return (
                  <button key={i} className="year-month"
                    aria-label={`进入 ${y2} 年 ${i + 1} 月`}
                    onClick={() => { setMonthOffset((y2 - now.getFullYear()) * 12 + i - now.getMonth()); setMode('month'); }}>
                    <span className="ym-name">{i + 1} 月</span>
                    <span className="ym-dots">{cnt > 0 && Array.from({ length: Math.min(cnt, 6) }, (_, j) => <i key={j} className="ym-dot" />)}</span>
                  </button>
                );
              })}
            </div>
          );
        })()}
        {mode === 'list' && (() => {
          /** DK-CALENDAR-04 任务 3: 日程列表页——全量按时间序+分组头+筛选+搜索。 */
          const [filtered] = [listFilter]; // 依赖标记（filter state 变化重渲染）
          const from = new Date(now.getFullYear(), now.getMonth(), now.getDate());
          from.setDate(from.getDate() - 7); // 含过去 7 天已完成
          const to = new Date(from);
          to.setDate(to.getDate() + 365);
          let insts = expandAll(events, from, to);
          if (listFilter === 'linked') insts = insts.filter((i) => i.ev.note_id != null);
          if (listFilter === 'birthday') insts = insts.filter((i) => i.ev.repeat?.lunar_month != null);
          if (listQuery.trim()) {
            const q = listQuery.trim().toLowerCase();
            insts = insts.filter((i) =>
              i.ev.title.toLowerCase().includes(q) || (i.ev.location ?? '').toLowerCase().includes(q));
          }
          insts.sort((a, b) => a.start.getTime() - b.start.getTime());
          /** 分组头: 今天/明天/本周/更晚（按本地日期键）。 */
          const groupOf = (d: Date): string => {
            const k = dKey2(d);
            if (k === todayStr) return '今天';
            const tmr = new Date(now); tmr.setDate(tmr.getDate() + 1);
            if (k === dKey2(tmr)) return '明天';
            const wk = new Date(now); wk.setDate(wk.getDate() + 6);
            if (d <= wk) return '本周';
            return '更晚';
          };
          const dKey2 = (d: Date) =>
            `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
          const groups: Array<{ label: string; items: typeof insts }> = [];
          for (const inst of insts) {
            const g = groupOf(inst.start);
            const last = groups[groups.length - 1];
            if (last && last.label === g) last.items.push(inst);
            else groups.push({ label: g, items: [inst] });
          }
          return (
            <div className="ev-list">
              <input className="input" placeholder="搜索标题/地点" value={listQuery}
                onChange={(e) => setListQuery(e.target.value)} aria-label="日程搜索" />
              <div className="ev-list-filters" role="tablist" aria-label="日程筛选">
                {([['all', '全部'], ['linked', '待办关联'], ['birthday', '生日']] as const).map(([v, label]) => (
                  <button key={v} className={`nt-seg ${listFilter === v ? 'on' : ''}`} role="tab"
                    aria-selected={listFilter === v} onClick={() => setListFilter(v)}>{label}</button>
                ))}
              </div>
              {groups.length === 0 && (
                <div className="empty-state"><div className="empty-text">无日程（{filtered === 'all' ? '全时间线' : '当前筛选'}）</div></div>
              )}
              {groups.map((g) => (
                <div key={g.label} className="ev-list-group">
                  <div className="ev-list-head">{g.label}</div>
                  {g.items.map((inst) => (
                    <button key={`${inst.ev.id}@${inst.offsetDays}`} className="ev-list-item" role="button"
                      onClick={() => setEventEditor(inst.ev)}>
                      <i className="ev-list-dot" style={{ background: EV_COLORS[inst.ev.color % 6] }} />
                      <span className="ev-list-time">{inst.ev.all_day ? '全天' : `${String(inst.start.getHours()).padStart(2, '0')}:${String(inst.start.getMinutes()).padStart(2, '0')}`}</span>
                      <span className="ev-list-title">{inst.ev.title}</span>
                      {inst.ev.note_id != null && <span className="ev-list-badge">关联</span>}
                      {inst.ev.repeat?.lunar_month != null && <span className="ev-list-badge">生日</span>}
                    </button>
                  ))}
                </div>
              ))}
            </div>
          );
        })()}
        {mode === 'day' && (
          <div className="day-axis" key={selected}>
            {/* 全天区：任务（due 无时刻）+ 全天事件 */}
            {(() => {
              const items = dayAxis.get(-1) ?? [];
              if (items.length === 0) return null;
              return (
                <div className="day-axis-row all-day">
                  <span className="day-axis-time">全天</span>
                  <div className="day-axis-items">
                    {items.map((it) => it.kind === 'event' && it.ev ? (
                      <span key={`e-${it.id}`} className="day-axis-item event" role="button" style={{
                        background: `rgba(${parseInt(EV_COLORS[it.ev.color % 6].slice(1, 3), 16)}, ${parseInt(EV_COLORS[it.ev.color % 6].slice(3, 5), 16)}, ${parseInt(EV_COLORS[it.ev.color % 6].slice(5, 7), 16)}, 0.15)`,
                        color: EV_COLORS[it.ev.color % 6],
                      }} onClick={() => setEventEditor(it.ev!)}>{it.label}</span>
                    ) : (
                      <span key={`t-${it.id}`} className="day-axis-item task" role="button"
                        onClick={() => onNewTask(selected)}>{it.label}</span>
                    ))}
                  </div>
                </div>
              );
            })()}
            {/* 08-22 时段 */}
            {Array.from({ length: 15 }, (_, i) => i + 8).map((h) => {
              const items = dayAxis.get(h) ?? [];
              return (
                <div key={h} className="day-axis-row">
                  <span className="day-axis-time">{String(h).padStart(2, '0')}:00</span>
                  <div className="day-axis-items">
                    {items.map((it) => it.kind === 'event' && it.ev ? (
                      <span key={`${it.kind}-${it.id}-${h}`} className="day-axis-item event" role="button" style={{
                        background: `rgba(${parseInt(EV_COLORS[it.ev.color % 6].slice(1, 3), 16)}, ${parseInt(EV_COLORS[it.ev.color % 6].slice(3, 5), 16)}, ${parseInt(EV_COLORS[it.ev.color % 6].slice(5, 7), 16)}, 0.15)`,
                        color: EV_COLORS[it.ev.color % 6],
                      }} onClick={() => setEventEditor(it.ev!)}>{it.label}</span>
                    ) : (
                      <span key={`${it.kind}-${it.id}-${h}`} className={`day-axis-item ${it.kind}`} role="button"
                        onClick={() => onOpenNote(it.id, it.label)}>{it.label}</span>
                    ))}
                  </div>
                </div>
              );
            })}
            {/* 轴外兜底区（8 点前/22 点后/无时刻解析失败）——Leo 10-29 合并为单「更早」/单「更晚」行 */}
            {(() => {
              const early: Array<{ kind: 'task' | 'note' | 'diary' | 'event'; label: string; id: string; key: string; ev?: CalEvent }> = [];
              const late: typeof early = [];
              for (const [h, items] of dayAxis.entries()) {
                if (h === -1 || (h >= 8 && h <= 22)) continue;
                (h < 8 ? early : late).push(...items.map((it) => ({ ...it, key: `${it.kind}-${it.id}-x` })));
              }
              type XItem = { kind: 'task' | 'note' | 'diary' | 'event'; label: string; id: string; key: string; ev?: CalEvent };
              const row = (label: string, arr: XItem[]) => arr.length === 0 ? null : (
                <div className="day-axis-row">
                  <span className="day-axis-time">{label}</span>
                  <div className="day-axis-items">
                    {arr.map((it: XItem) => it.kind === 'event' && it.ev ? (
                      <span key={it.key} className="day-axis-item event" role="button" style={{
                        background: `rgba(${parseInt(EV_COLORS[it.ev.color % 6].slice(1, 3), 16)}, ${parseInt(EV_COLORS[it.ev.color % 6].slice(3, 5), 16)}, ${parseInt(EV_COLORS[it.ev.color % 6].slice(5, 7), 16)}, 0.15)`,
                        color: EV_COLORS[it.ev.color % 6],
                      }} onClick={() => setEventEditor(it.ev!)}>{it.label}</span>
                    ) : (
                      <span key={it.key} className={`day-axis-item ${it.kind}`} role="button"
                        onClick={() => onOpenNote(it.id, it.label)}>{it.label}</span>
                    ))}
                  </div>
                </div>
              );
              return <>{row('更早', early)}{row('更晚', late)}</>;
            })()}
          </div>
        )}
        {mode === 'month' && selected !== todayStr && (
          <button className="btn btn-text btn-block" style={{ marginTop: 4 }} onClick={() => setSelected(todayStr)}>
            回到今天
          </button>
        )}
      </div>

      {/* 当日聚合 */}
      <div className="result-section-title" style={{ marginTop: 12 }}>
        {selected.replace(/^(\d+)-(\d+)-(\d+)$/, '$1 年 $2 月 $3 日')}
        <span style={{ marginLeft: 8, fontSize: 13, fontWeight: 400, opacity: 0.7 }}>
          {selLunarInfo.text}{selLunarInfo.tag ? ` · ${selLunarInfo.tag}` : ''}
        </span>
      </div>

      {/* DK-CALENDAR-01 黄历宜忌 */}
      {(selLunarInfo.yi.length > 0 || selLunarInfo.ji.length > 0) && (
        <div className="almanac">
          {selLunarInfo.yi.length > 0 && (
            <div className="almanac-row"><span className="almanac-tag yi">宜</span>{selLunarInfo.yi.map((s) => <span key={s} className="almanac-item">{s}</span>)}</div>
          )}
          {selLunarInfo.ji.length > 0 && (
            <div className="almanac-row"><span className="almanac-tag ji">忌</span>{selLunarInfo.ji.map((s) => <span key={s} className="almanac-item">{s}</span>)}</div>
          )}
        </div>
      )}

      {isSelInMonth && (selData.dayTasks.length > 0 || (selData.dayNotes.length + selData.dayDiary.length) > 0) === false && (
        <EmptyState text="这一天没有任务或笔记记录。点右下角 + 捕获。" />
      )}

      {selData.dayTasks.length > 0 && (
        <div className="card" style={{ padding: 4, marginBottom: 8 }}>
          {selData.dayTasks.map((t) => (
            <div key={t.id} className="task-item">
              <span className={`task-check ${t.status === 'done' ? 'checked' : ''}`}>{I.check}</span>
              <div className="task-body">
                <div className="task-text">{t.text}</div>
                <div className="task-meta"><span className="task-status-tag">{TASK_GROUPS.find((g) => g.id === t.status)?.label}</span></div>
              </div>
            </div>
          ))}
        </div>
      )}

      {isSelInMonth && [...selData.dayDiary, ...selData.dayNotes].length > 0 && (
        <div className="note-list">
          {[...selData.dayDiary, ...selData.dayNotes.filter((n) => !selData.dayDiary.some((x) => x.id === n.id))].map((n) => (
            <NoteCard key={n.id} note={n} onOpen={onOpenNote} />
          ))}
        </div>
      )}

      <button className="btn btn-secondary btn-block" style={{ marginTop: 8 }} onClick={() => onNewTask(selected)}>
        {I.plus} 为这一天添加任务
      </button>

      {/* DK-CALENDAR-02 任务 3：日程入口（选中日预填 start）。 */}
      <button className="btn btn-primary btn-block" style={{ marginTop: 8 }} onClick={() => setEventEditor({
        id: '', title: '', start: `${selected}T09:00:00`, end: null,
        all_day: false, location: null, note_id: null, color: 1,
      })}>
        {I.calendar} 为这一天添加日程
      </button>

      {/* DK-CALENDAR-02：事件编辑 sheet（新建/编辑共用——存/删/关联笔记跳转）。 */}
      {eventEditor && (
        <EventEditorSheet
          ev={eventEditor}
          onClose={() => setEventEditor(null)}
          onSave={(ev) => { onUpsertEvent(ev); setEventEditor(null); }}
          onDelete={(id) => { onDeleteEvent(id); setEventEditor(null); }}
          onOpenNote={onOpenNote}
        />
      )}
    </div>
  );
}

/** DK-CALENDAR-02：事件编辑 sheet。
 * 字段面（P0）：标题/起止 datetime-local/全天/色 0-5/地点/关联笔记。
 * datetime-local 值「YYYY-MM-DDTHH:mm」↔ RFC3339 本地→ISO 转换。 */
function EventEditorSheet({ ev, onClose, onSave, onDelete, onOpenNote }: {
  ev: CalEvent;
  onClose: () => void;
  onSave: (ev: CalEvent) => void;
  onDelete: (id: string) => void;
  onOpenNote: (id: string, title: string) => void;
}) {
  /** RFC3339→datetime-local（本地时区显示——Leo 8f82772 时区纪律）。 */
  const toLocalInput = (iso: string): string => {
    const d = new Date(iso);
    if (Number.isNaN(d.getTime())) return '';
    const pad = (n: number) => String(n).padStart(2, '0');
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
  };
  const [title, setTitle] = useState(ev.title);
  const [start, setStart] = useState(toLocalInput(ev.start));
  const [end, setEnd] = useState(ev.end ? toLocalInput(ev.end) : '');
  const [allDay, setAllDay] = useState(ev.all_day);
  const [location, setLocation] = useState(ev.location ?? '');
  const [color, setColor] = useState(ev.color % 6);
  // DK-CALENDAR-03: 重复 UI 状态（回填自 ev.repeat）
  const [freq, setFreq] = useState<'none' | 'daily' | 'weekly' | 'monthly' | 'yearly' | 'yearly_lunar'>(
    ev.repeat ? (ev.repeat.lunar_month != null ? 'yearly_lunar' : ev.repeat.freq) : 'none');
  const [interval, setInterval] = useState(Math.max(1, ev.repeat?.interval ?? 1));
  const [weekdays, setWeekdays] = useState<number[]>(ev.repeat?.by_weekday ?? []);
  const [count, setCount] = useState(ev.repeat?.count?.toString() ?? '');
  // DK-CALENDAR-04 任务 5: 提醒段（多档勾选+自定义分钟+闹钟级）。
  const [reminders, setReminders] = useState<ReminderSetting[]>(ev.reminders ?? []);
  const toggleReminder = (min: number) => {
    setReminders((rs) => rs.some((r) => r.offset_min === min)
      ? rs.filter((r) => r.offset_min !== min)
      : [...rs, { offset_min: min, alarm_level: 0 }]);
  };
  const toggleAlarm = (min: number) => {
    setReminders((rs) => rs.map((r) => r.offset_min === min ? { ...r, alarm_level: r.alarm_level > 0 ? 0 : 1 } : r));
  };
  const isNew = !ev.id;
  const EV_COLORS = ['#dc2626', '#2563eb', '#059669', '#d97706', '#7c3aed', '#6b7280'];
  const WD = ['一', '二', '三', '四', '五', '六', '日'];

  const save = () => {
    if (!title.trim() || !start) return;
    const startIso = new Date(start).toISOString();
    const endIso = end ? new Date(end).toISOString() : null;
    // end<start fail-closed（core A04 同语义——防真桥拒绝后 UI 静默丢:
    // MobileApp onUpsertEvent 不看返回值,前置守卫是唯一防线）
    if (endIso && endIso < startIso) return;
    // 组装 repeat（03: 农历模式由锚点反推 lunar_month/lunar_day——锚点即生日）
    let repeat: CalEvent['repeat'] = null;
    if (freq !== 'none') {
      if (freq === 'yearly_lunar') {
        const anchor = new Date(start);
        const lunar = Lunar.fromDate(anchor);
        repeat = {
          freq: 'yearly', interval: Math.max(1, interval), until: null,
          lunar_month: lunar.getMonth(), lunar_day: lunar.getDay(),
        };
      } else {
        repeat = {
          freq, interval: Math.max(1, interval), until: null,
          by_weekday: freq === 'weekly' ? (weekdays.length ? [...weekdays].sort((a, b) => a - b) : null) : null,
          count: count.trim() ? Math.max(1, parseInt(count, 10) || 1) : null,
        };
      }
    }
    onSave({
      ...ev,
      title: title.trim(),
      start: startIso,
      end: endIso,
      all_day: allDay,
      location: location.trim() || null,
      color,
      repeat,
      reminders: reminders.length ? reminders.sort((a, b) => a.offset_min - b.offset_min) : null,
    });
  };

  return (
    <>
      <div className="sheet-mask" onClick={onClose} />
      <div className="sheet">
        <div className="sheet-grab" />
        <div className="sheet-head">
          <span className="sheet-title">{isNew ? '新建日程' : '编辑日程'}</span>
          <button className="icon-btn" onClick={onClose} aria-label="关闭">{I.x}</button>
        </div>
        <div className="sheet-body">
          <input className="input" placeholder="日程标题" value={title}
            onChange={(e) => setTitle(e.target.value)} aria-label="日程标题" />
          <label className="ev-field">
            <span>开始</span>
            <input type="datetime-local" value={start} onChange={(e) => setStart(e.target.value)} aria-label="开始时间" />
          </label>
          <label className="ev-field">
            <span>结束</span>
            <input type="datetime-local" value={end} onChange={(e) => setEnd(e.target.value)} aria-label="结束时间" />
          </label>
          <label className="ev-field ev-check">
            <span>全天</span>
            <input type="checkbox" checked={allDay} onChange={(e) => setAllDay(e.target.checked)} aria-label="全天事件" />
          </label>
          <label className="ev-field">
            <span>地点</span>
            <input className="input" placeholder="可选" value={location}
              onChange={(e) => setLocation(e.target.value)} aria-label="地点" />
          </label>
          <div className="ev-field">
            <span>颜色</span>
            <div className="ev-colors" role="radiogroup" aria-label="日程颜色">
              {EV_COLORS.map((c, i) => (
                <button key={i} type="button" className={`ev-color ${i === color ? 'sel' : ''}`}
                  style={{ background: c }} role="radio" aria-checked={i === color}
                  onClick={() => setColor(i)} aria-label={`颜色 ${i + 1}`} />
              ))}
            </div>
          </div>
          {/* DK-CALENDAR-03: 重复规则段。 */}
          <div className="ev-field">
            <span>重复</span>
            <div className="ev-freq-seg" role="radiogroup" aria-label="重复频率">
              {([
                ['none', '不重复'], ['daily', '每天'], ['weekly', '每周'],
                ['monthly', '每月'], ['yearly', '每年'], ['yearly_lunar', '农历每年'],
              ] as const).map(([v, label]) => (
                <button key={v} type="button" className={`ev-freq ${freq === v ? 'sel' : ''}`}
                  role="radio" aria-checked={freq === v} onClick={() => setFreq(v)}>{label}</button>
              ))}
            </div>
          </div>
          {freq !== 'none' && (
            <label className="ev-field">
              <span>间隔</span>
              <span className="ev-interval">每
                <input type="number" min={1} max={99} value={interval}
                  onChange={(e) => setInterval(Math.max(1, parseInt(e.target.value, 10) || 1))}
                  aria-label="重复间隔" />
                {freq === 'daily' ? '天' : freq === 'weekly' ? '周' : freq === 'monthly' ? '月' : '年'}
              </span>
            </label>
          )}
          {freq === 'weekly' && (
            <div className="ev-field">
              <span>周几</span>
              <div className="ev-weekdays" role="group" aria-label="重复周几">
                {WD.map((w, i) => (
                  <button key={i} type="button" className={`ev-wd ${weekdays.includes(i) ? 'sel' : ''}`}
                    aria-pressed={weekdays.includes(i)}
                    onClick={() => setWeekdays((ws) => ws.includes(i) ? ws.filter((x) => x !== i) : [...ws, i])}>
                    {w}
                  </button>
                ))}
              </div>
            </div>
          )}
          {freq !== 'none' && freq !== 'yearly_lunar' && (
            <label className="ev-field">
              <span>次数</span>
              <input type="number" min={1} placeholder="不限" value={count}
                onChange={(e) => setCount(e.target.value)} aria-label="重复次数（空=不限）" />
            </label>
          )}
          {freq === 'yearly_lunar' && start && (
            <div className="ev-lunar-hint">
              按农历 {Lunar.fromDate(new Date(start)).toString()} 每年重复（闰年无对应闰月取次月同日）
            </div>
          )}
          {/* DK-CALENDAR-04 任务 5: 提醒段——多档勾选+自定义+闹钟级。 */}
          <div className="ev-field">
            <span>提醒</span>
            <div className="ev-reminders" role="group" aria-label="提醒档位">
              {[5, 30, 60, 1440].map((min) => {
                const on = reminders.some((r) => r.offset_min === min);
                const alarm = reminders.some((r) => r.offset_min === min && r.alarm_level > 0);
                return (
                  <span key={min} className={`ev-rem ${on ? 'sel' : ''}`}>
                    <button type="button" className="ev-rem-btn"
                      aria-pressed={on} onClick={() => toggleReminder(min)}>
                      {min === 1440 ? '1 天' : `${min} 分钟`}
                    </button>
                    {on && (
                      <button type="button" className={`ev-rem-alarm ${alarm ? 'on' : ''}`}
                        aria-pressed={alarm} title="闹钟级（Doze 准时）"
                        aria-label={`${min === 1440 ? '1天' : min + '分钟'}闹钟级提醒`}
                        onClick={() => toggleAlarm(min)}>铃</button>
                    )}
                  </span>
                );
              })}
            </div>
          </div>
          <label className="ev-field">
            <span>自定义</span>
            <span className="ev-interval">提前
              <input type="number" min={0} placeholder="分钟" aria-label="自定义提前分钟"
                onKeyDown={(e) => {
                  if (e.key === 'Enter') {
                    const v = parseInt((e.target as HTMLInputElement).value, 10);
                    if (!Number.isNaN(v) && v >= 0) { toggleReminder(v); (e.target as HTMLInputElement).value = ''; }
                  }
                }} />
              分钟回车
            </span>
          </label>
          {reminders.length > 0 && (
            <div className="ev-lunar-hint">
              已设 {reminders.length} 档: {reminders.map((r) => `${r.offset_min === 1440 ? '1天' : r.offset_min + '分'}${r.alarm_level > 0 ? '(铃)' : ''}`).join(' / ')}
            </div>
          )}
          {ev.note_id && (
            <button className="btn btn-text btn-block" onClick={() => onOpenNote(ev.note_id!, ev.title)}>
              {I.notes} 打开关联笔记
            </button>
          )}
          <button className="btn btn-primary btn-block" style={{ marginTop: 8 }} onClick={save}
            disabled={!title.trim() || !start}>
            保存日程
          </button>
          {!isNew && (
            <button className="btn btn-danger-text btn-block" style={{ marginTop: 8 }}
              onClick={() => { if (window.confirm('彻底删除该日程？')) onDelete(ev.id); }}>
              删除日程
            </button>
          )}
        </div>
      </div>
    </>
  );
}
