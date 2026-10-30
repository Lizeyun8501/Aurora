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
import { Solar, HolidayUtil } from 'lunar-typescript';
import {
  type Task, TASK_GROUPS, I, listNotesTitled, NoteCard, EmptyState,
} from '../MobileApp';
import type { CalEvent } from '../adapters/androidPlatform';


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
  /** DK-CALENDAR-01 任务 2：月/日双视图 segmented */
  const [mode, setMode] = useState<'month' | 'day'>('month');
  /** DK-CALENDAR-02：事件编辑器（null=关;编辑/新建共用 sheet）。 */
  const [eventEditor, setEventEditor] = useState<CalEvent | null>(null);

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
    // DK-CALENDAR-02 任务 3：月历日格事件角标（当月日格含事件）
    const key = `${y}-${String(m + 1).padStart(2, '0')}-${String(d).padStart(2, '0')}`;
    return events.some((e) => {
      const sKey = eventLocalKey(e.start);
      const eKey = e.end ? eventLocalKey(e.end) : sKey;
      return sKey <= key && key <= eKey;
    });
  };

  /** DK-CALENDAR-02 任务 3：日格事件色点列表（最多 3 色，多的省略号）。 */
  const dayEventDots = (d: number): number[] => {
    const key = `${y}-${String(m + 1).padStart(2, '0')}-${String(d).padStart(2, '0')}`;
    const hits = events.filter((e) => {
      const sKey = eventLocalKey(e.start);
      const eKey = e.end ? eventLocalKey(e.end) : sKey;
      return sKey <= key && key <= eKey;
    });
    return [...new Set(hits.map((e) => e.color % 6))].slice(0, 3);
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

  /** DK-CALENDAR-02：事件工具——本地日期键（UTC RFC3339→本地 YYYY-MM-DD;
   * 时区纪律：所有日界/小时换算一律 Date 本地解析——Leo 8f82772 教训）。 */
  const eventLocalKey = (iso: string): string => {
    const d = new Date(iso);
    if (Number.isNaN(d.getTime())) return '';
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
  };

  /** 选中日事件（all_day 按本地 start 日;跨天事件按交叠——P0 按日含 start/end 落格）。 */
  const selEvents = events.filter((e) => {
    const s = new Date(e.start);
    if (Number.isNaN(s.getTime())) return false;
    const sKey = eventLocalKey(e.start);
    const eKey = e.end ? eventLocalKey(e.end) : sKey;
    return selected >= sKey && selected <= eKey;
  });

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
    type AxisItem = { kind: 'task' | 'note' | 'diary' | 'event'; label: string; id: string; ev?: CalEvent };
    const slots = new Map<number, AxisItem[]>();
    const push = (h: number, item: AxisItem) => {
      const arr = slots.get(h) ?? [];
      arr.push(item);
      slots.set(h, arr);
    };
    for (const t of selData.dayTasks) push(-1, { kind: 'task', label: t.text, id: t.id });
    for (const n of selData.dayDiary) push(hour(n.updatedAt), { kind: 'diary', label: n.title, id: n.id });
    for (const n of selData.dayNotes) push(hour(n.updatedAt), { kind: 'note', label: n.title, id: n.id });
    // DK-CALENDAR-02 任务 4 聚合混排：事件色块（all_day→全天区;时刻事件按 start 本地小时）
    for (const e of selEvents) {
      const h = e.all_day ? -1 : hour(e.start);
      push(h, { kind: 'event', label: e.title, id: e.id, ev: e });
    }
    return slots;
  })();

  return (
    <div className="view">
      <div className="card">
        <div className="cal-head">
          <button className="icon-btn" onClick={() => setMonthOffset((o) => o - 1)} aria-label="上月">{I.back}</button>
          <span className="cal-title">{y} 年 {m + 1} 月</span>
          <button className="icon-btn" onClick={() => setMonthOffset((o) => o + 1)} aria-label="下月">
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round"><path d="m9 18 6-6-6-6"/></svg>
          </button>
          <div className="cal-mode-seg" role="tablist" aria-label="日历视图切换">
            <button className={`nt-seg ${mode === 'month' ? 'on' : ''}`} role="tab" aria-selected={mode === 'month'} onClick={() => setMode('month')}>月</button>
            <button className={`nt-seg ${mode === 'day' ? 'on' : ''}`} role="tab" aria-selected={mode === 'day'} onClick={() => setMode('day')}>日</button>
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
  const isNew = !ev.id;
  const EV_COLORS = ['#dc2626', '#2563eb', '#059669', '#d97706', '#7c3aed', '#6b7280'];

  const save = () => {
    if (!title.trim() || !start) return;
    const startIso = new Date(start).toISOString();
    const endIso = end ? new Date(end).toISOString() : null;
    onSave({
      ...ev,
      title: title.trim(),
      start: startIso,
      end: endIso,
      all_day: allDay,
      location: location.trim() || null,
      color,
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
