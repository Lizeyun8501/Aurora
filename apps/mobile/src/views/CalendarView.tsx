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


export default function CalendarView({ tasks, onOpenNote, onNewTask, initialSelected }: {
  tasks: Task[];
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
    const dayNotes = notes.filter((n) => n.updatedAt.startsWith(key));
    const dayDiary = notes.filter((n) => n.title.includes(key) && n.title.includes('日记'));
    const dayTasks = tasks.filter((t) => t.due === key);
    return { dayNotes, dayDiary, dayTasks };
  };
  const hasMark = (d: number) => {
    const { dayNotes, dayDiary, dayTasks } = dayData(d);
    return dayNotes.length > 0 || dayDiary.length > 0 || dayTasks.length > 0;
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

  return (
    <div className="view">
      <div className="card">
        <div className="cal-head">
          <button className="icon-btn" onClick={() => setMonthOffset((o) => o - 1)} aria-label="上月">{I.back}</button>
          <span className="cal-title">{y} 年 {m + 1} 月</span>
          <button className="icon-btn" onClick={() => setMonthOffset((o) => o + 1)} aria-label="下月">
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round"><path d="m9 18 6-6-6-6"/></svg>
          </button>
        </div>
        <div className="cal-grid">
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
              </button>
            );
          })}
        </div>
        {selected !== todayStr && (
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
    </div>
  );
}
