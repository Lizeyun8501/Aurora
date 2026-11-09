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
import React, { useEffect, useRef, useState } from 'react';
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
  /* 11-04 19:20 ①: 24h 轴滚动定位——周/日模式+换 selected 时滚到当前时段。
     11-06 17:29 ②: 红线居屏优先——滚到 now 上下文中心（容不下则 clamp 至少可见）,不再需要滑动找线。 */
  const axisHoursRef = useRef<HTMLDivElement | null>(null);
  const weekAxisRef = useRef<HTMLDivElement | null>(null);
  useEffect(() => {
    const d = new Date(); const liveMin = d.getHours() * 60 + d.getMinutes();
    if (selected === todayStr) setNowMin(liveMin);
    const scrollToCenter = (el: HTMLElement | null, anchorTop: number) => {
      if (!el) return;
      const target = anchorTop - el.clientHeight / 2 + 24; /* +24=行半高,线居行 */
      const max = Math.max(0, el.scrollHeight - el.clientHeight);
      el.scrollTo({ top: Math.max(0, Math.min(target, max)), behavior: 'instant' as ScrollBehavior });
    };
    const nowTop = 48 + (liveMin * 48) / 60; /* 顶空格 +48 基 */
    if (mode === 'week') scrollToCenter(weekAxisRef.current, nowTop); /* 周页红线恒今——无条件滚红线 */
    if (mode === 'day') scrollToCenter(axisHoursRef.current, selected === todayStr ? nowTop : 48 + 8 * 48); /* 他日=08:00 惯例 */
  }, [mode, selected]);
  /** DK-CALENDAR-02：事件编辑器（null=关;编辑/新建共用 sheet）。 */
  const [eventEditor, setEventEditor] = useState<CalEvent | null>(null);
  /* 11-05 08:40 ② 点时段→蓝框+加号;③ 加号开简要版快建（更多设置跳完整版）。
     11-09 17:46 用户令④(纠偏): 周页 24h 轴时间格(横时间线×纵日期列交织格)点选→格框+加号快建——
     weekSlot={col,h};quickDay=快建日期(周页=点击列日,日页=selected)。 */
  const [slotSel, setSlotSel] = useState<number | null>(null);
  const [weekSlot, setWeekSlot] = useState<{ col: number; h: number } | null>(null);
  const [quickNew, setQuickNew] = useState<number | null>(null);
  const [quickDay, setQuickDay] = useState<string | null>(null);
  const [quickTitle, setQuickTitle] = useState('');
  const setSlotSlot = (h: number | null) => setSlotSel(h);
  const closeQuick = () => { setQuickNew(null); setQuickDay(null); };
  const dayIso = (h: number) => {
    const [y2, m2, d2] = (quickDay ?? selected).split('-').map(Number);
    const end = new Date(y2, m2 - 1, d2, h + 1);
    const pad = (n: number) => String(n).padStart(2, '0');
    return {
      start: `${y2}-${pad(m2)}-${pad(d2)}T${pad(h)}:00:00`,
      end: `${end.getFullYear()}-${pad(end.getMonth() + 1)}-${pad(end.getDate())}T${pad(end.getHours())}:00:00`,
      mLabel: `${m2}月${d2}日`,
      endLabel: `${end.getMonth() + 1}月${end.getDate()}日`,
    };
  };
  /** 21:40 用户指令②: 年视图滑动切年（上滑=下一年/下滑=过去一年）。 */
  const [yearDelta, setYearDelta] = useState(0);
  const yearTouch = useRef<{ x: number; y: number } | null>(null);
  /* 11-03 08:31 报障①: 月页签无滑动翻月（用户滑了无反应→「今」永不出现）——补双通道绑（pointer+touchend 兜底,同年页模式）。 */
  const monthTouch = useRef<{ x: number; y: number } | null>(null);
  /* 11-08 22:01 ③: 周/日页签左右滑动翻页（用户令）——周±7 天/日±1 天。
     水平位移>48 且 |dx|>|dy|×1.2（滤上下滚轴手势）;左滑=前进,右滑=回退（同翻页自然语义）。 */
  const calSwipe = useRef<{ x: number; y: number } | null>(null);
  /** DK-CALENDAR-04 任务 3: 列表页筛选+搜索。 */
  const [listFilter, setListFilter] = useState<'all' | 'linked' | 'birthday'>('all');
  const [listQuery, setListQuery] = useState('');
  /** CALENDAR-VIVO（10-31）: 当前时刻分钟数——日视图红时间线（30s 刷新;480/1320=08:00/22:00 轴界。 */
  const [nowMin, setNowMin] = useState(() => { const d = new Date(); return d.getHours() * 60 + d.getMinutes(); });
  useEffect(() => {
    const t = setInterval(() => { const d = new Date(); setNowMin(d.getHours() * 60 + d.getMinutes()); }, 30_000);
    /* 11-04 19:51 ③: 真机 webview 后台冻结 interval——可见性恢复/焦点回归立即刷,红线恒实时。 */
    const refresh = () => { const d = new Date(); setNowMin(d.getHours() * 60 + d.getMinutes()); };
    document.addEventListener('visibilitychange', refresh);
    window.addEventListener('focus', refresh);
    return () => { clearInterval(t); document.removeEventListener('visibilitychange', refresh); window.removeEventListener('focus', refresh); };
  }, []);

  /* 11-08 08:41 ①注: now 三角定位无需 JS——dy css 常数直写(.now-tri): DPR3 真机 -2×3=-6 物理px
     天然整数(量化零损耗);SVG path AA 栅格化亚像素连续,0.1px 级 dy 微调真实生效(旧 border 三角
     top 小数被 paint 取整吞掉的黑洞根除)。残余系统差(旧 border 实证 +1 物理行): PIL 扫一次补 dy。 */

  const view = new Date(now.getFullYear(), now.getMonth() + monthOffset, 1);
  /* 11-02 23:33 用户指令: 五页签滑离当前时段→右下「今」透明圆（加号上方并排）——一点全归位。
     显示条件按模式: 年=yearDelta 残;月=monthOffset 翻走;周/日=selected 非 today（view 由 selected 驱动）;日程=筛选/搜索态。 */
  /* 11-03 08:42: 月页点远日期（含同月远日）也显「今」——下方「回到今天」按钮已删,右下角成唯一返回今天入口。 */
  const offToday = mode === 'year' ? yearDelta !== 0
    : mode === 'month' ? monthOffset !== 0 || selected !== todayStr
    : mode === 'week' || mode === 'day' ? selected !== todayStr
    : mode === 'list' ? (listFilter !== 'all' || listQuery !== '')
    : false;
  const goToday = () => { setMonthOffset(0); setSelected(todayStr); setYearDelta(0); setListFilter('all'); setListQuery(''); };
  const y = view.getFullYear();
  const m = view.getMonth();
  const lead = new Date(y, m, 1).getDay(); // 11-04 08:03 周日起始（与年页/系统历统一;11 月首行满排消首行大空白）
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
    /* 11-09 07:31 用户令: 日下点=仅日程或待办——原 dayNotes/dayDiary 也算,
       今日无日程待办仅日记/笔记也出红点=误报（「为何今日 9 号下面会有红点」根因）。
       日程点在月历 JSX dayEventDots 独立管;此处只管待办。 */
    const key = `${y}-${String(m + 1).padStart(2, '0')}-${String(d).padStart(2, '0')}`;
    return tasks.some((t) => t.due === key);
  };

  /** DK-CALENDAR-02 任务 3 / 03 展开面：日格事件色点列表（最多 3 色）。 */
  const dayEventDots = (d: number): number[] => {
    const key = `${y}-${String(m + 1).padStart(2, '0')}-${String(d).padStart(2, '0')}`;
    const hits = monthInstances.filter((i) => eventLocalKey(i.start.toISOString()) === key);
    return [...new Set(hits.map((i) => i.ev.color % 6))].slice(0, 3);
  };

  // 农历/节假日 — lunar-typescript（6tail, 含法定节假日+调休数据随库更新）
  /** 格子副标: 法定假日名 > 农历节日 > 公历节日 > 节气 > 初一显月名 > 农历日。 */
  const lunarSub = (d: number, yy = y, mm = m): { sub: string; isStat: boolean; isWork: boolean } => {
    const solar = Solar.fromYmd(yy, mm + 1, d);
    const lunar = solar.getLunar();
    const h = HolidayUtil.getHoliday(yy, mm + 1, d);
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
    <div
      className={'view cal-fullview' + (mode === 'year' ? ' cal-year-fill' : '') + (mode === 'week' || mode === 'day' ? ' cal-scroll-mode' : '')}
      /* 11-08 22:01 ③: 周/日左右滑翻页——绑 view 容器（覆盖列头+轴全域;竖滚被 |dx|>|dy| 滤波滤除）。 */
      onPointerDown={(e) => { if (mode === 'week' || mode === 'day') calSwipe.current = { x: e.clientX, y: e.clientY }; }}
      onPointerUp={(e) => {
        if (mode !== 'week' && mode !== 'day') return;
        const t = calSwipe.current; if (!t) return;
        calSwipe.current = null;
        const dx = e.clientX - t.x, dy = e.clientY - t.y;
        if (Math.abs(dx) > 48 && Math.abs(dx) > Math.abs(dy) * 1.2) {
          const step = mode === 'week' ? 7 : 1;
          const d = new Date(`${selected}T00:00:00`); d.setDate(d.getDate() + (dx < 0 ? step : -step));
          setSelected(`${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`);
        }
      }}
      onPointerCancel={() => { calSwipe.current = null; }}
      onTouchMove={(e) => {
        /* 11-09 15:39 用户令: 真机左右滑无反应根修——touchmove 即时判定(不等 touchend:
           指针流被滚动手势 cancel/个别 ROM 吞 touchend,move 必达;跟手翻页)。
           触发即清 calSwipe=消费,touchend 兜底自动 no-op。 */
        if (mode !== 'week' && mode !== 'day') return;
        const t = calSwipe.current; if (!t) return;
        const tc = e.touches[0]; if (!tc) return;
        const dx = tc.clientX - t.x, dy = tc.clientY - t.y;
        if (Math.abs(dx) > 48 && Math.abs(dx) > Math.abs(dy) * 1.2) {
          calSwipe.current = null;
          const step = mode === 'week' ? 7 : 1;
          const d = new Date(`${selected}T00:00:00`); d.setDate(d.getDate() + (dx < 0 ? step : -step));
          setSelected(`${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`);
        }
      }}
      onTouchEnd={(e) => {
        /* 双通道兜底: pointer 流个别 ROM 被吞时 touchend 保底（同月页）。 */
        if (mode !== 'week' && mode !== 'day') return;
        const t = calSwipe.current; if (!t) return;
        const tc = e.changedTouches[0]; if (!tc) return;
        calSwipe.current = null;
        const dx = tc.clientX - t.x, dy = tc.clientY - t.y;
        if (Math.abs(dx) > 48 && Math.abs(dx) > Math.abs(dy) * 1.2) {
          const step = mode === 'week' ? 7 : 1;
          const d = new Date(`${selected}T00:00:00`); d.setDate(d.getDate() + (dx < 0 ? step : -step));
          setSelected(`${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`);
        }
      }}
    >
      <div className="card cal-card">
        <div className="cal-head">
          {/* 11-03 17:05 ④: 左右箭头已删——月/年变换统一上下滑（cal-grid/year-vivo 双通道滑绑）。 */}
          {mode === 'year' ? (
            <span className="cal-title cal-title-year" role="button" aria-label="返回月视图"
              onClick={() => setMode('month')}><span className="cal-title-y">{String(y + yearDelta).slice(0, 2)}</span><b className={'cal-title-m' + (y + yearDelta === now.getFullYear() ? ' cur' : '')}>{String(y + yearDelta).slice(2)}</b></span>
          ) : (
            <span className="cal-title" role="button" aria-label="查看年视图"
              onClick={() => { setYearDelta(0); setMode('year'); }}><span className="cal-title-y">{y}</span> 年 <b className={'cal-title-m' + (monthOffset === 0 ? ' cur' : '')}>{m + 1}</b> 月</span>
          )}
        </div>
        {/* ② CALENDAR-VIVO 11-01 20:22: 年|月|周|日|日程 胶囊条独立行（年月下通栏——原挤标题行右侧） */}
        <div className="cal-mode-seg" role="tablist" aria-label="日历视图切换">
          <button className={`nt-seg ${mode === 'year' ? 'on' : ''}`} role="tab" aria-selected={mode === 'year'} onClick={() => { setYearDelta(0); setMode('year'); }}>年</button>
          <button className={`nt-seg ${mode === 'month' ? 'on' : ''}`} role="tab" aria-selected={mode === 'month'} onClick={() => setMode('month')}>月</button>
          <button className={`nt-seg ${mode === 'week' ? 'on' : ''}`} role="tab" aria-selected={mode === 'week'} onClick={() => setMode('week')}>周</button>
          <button className={`nt-seg ${mode === 'day' ? 'on' : ''}`} role="tab" aria-selected={mode === 'day'} onClick={() => setMode('day')}>日</button>
          <button className={`nt-seg ${mode === 'list' ? 'on' : ''}`} role="tab" aria-selected={mode === 'list'} onClick={() => setMode('list')}>日程</button>
        </div>
        {mode === 'month' && (
          <div
            className="cal-grid" key={monthOffset}
            onPointerDown={(e) => { monthTouch.current = { x: e.clientX, y: e.clientY }; }}
            onPointerUp={(e) => {
              const t = monthTouch.current; if (!t) return;
              const dx = e.clientX - t.x, dy = e.clientY - t.y;
              monthTouch.current = null;
              if (Math.abs(dy) > 64 && Math.abs(dx) < 60) setMonthOffset((o) => o + (dy < 0 ? 1 : -1));
            }}
            onPointerCancel={() => { monthTouch.current = null; }}
            onTouchEnd={(e) => {
              /* 双通道兜底: pointer 流个别 ROM 被吞时 touchend 保底（同年页） */
              const t = monthTouch.current; if (!t) return;
              const tc = e.changedTouches[0]; if (!tc) return;
              const dx = tc.clientX - t.x, dy = tc.clientY - t.y;
              monthTouch.current = null;
              if (Math.abs(dy) > 64 && Math.abs(dx) < 60) setMonthOffset((o) => o + (dy < 0 ? 1 : -1));
            }}
          >
            {['日', '一', '二', '三', '四', '五', '六'].map((w) => (
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
                  {/* 11-08 23:10 用户令: 有日程或待办→仅一种颜色的单点（事件取其色 EV_COLORS[号],纯任务/笔记=accent 红）
                      ——替换原 cal-ev-dots 多色点列+has::after 红点并列（「红点和另外一个并列显示有点怪」根除）。 */}
                  {(dayEventDots(d).length > 0 || hasMark(d)) && (
                    <i className="cal-day-dot" style={{ background: dayEventDots(d).length > 0 ? EV_COLORS[dayEventDots(d)[0]] : 'var(--cal-accent)' }} />
                  )}
                </button>
              );
            })}
          </div>
        )}
        {(mode === 'week' || mode === 'day') && (() => {
          /** DK-CALENDAR-04 任务 1: 周视图——7 天并排+当日实例色块列。
           * 周界=selected 所在周一;列头点入切日视图。
           * 11-08 22:01 用户令: ①「日一二三四五六」行独立上移紧贴页签（原挤列头首行）;
           * ②列头=数字+农历,sel 框含农历（原 48 框只罩 wd+num,农历特意推出框外——现并入）。 */
          const wStart = new Date(`${selected}T00:00:00`);
          /* 11-04 19:20 ②: 周日为首（同月历 v27.27 语义统一）。 */
          wStart.setDate(wStart.getDate() - wStart.getDay());
          const days = Array.from({ length: 7 }, (_, i) => {
            const d = new Date(wStart);
            d.setDate(d.getDate() + i);
            return d;
          });
          const WD = ['日', '一', '二', '三', '四', '五', '六'];
          const pad = (n: number) => String(n).padStart(2, '0');
          const dKey = (d: Date) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
          return (
            <>
              {/* 周几条: 紧贴年月周日日程页签下,7 列与下方 week-grid 同列对齐（margin-left 同步 20）。 */}
              <div className="week-wd-strip" aria-hidden="true" key={'ws-' + dKey(wStart)}>
                {days.map((d, i) => <span key={dKey(d)}>{WD[i]}</span>)}
              </div>
              <div className="week-grid" key={'wg-' + dKey(wStart)}>
              {days.map((d, i) => {
                const k = dKey(d);
                const insts = expandAll(events, new Date(`${k}T00:00:00`), new Date(`${k}T23:59:59`));
                const isToday = k === todayStr;
                return (
                  <div key={k} className={`week-col ${isToday ? 'today' : ''} ${k === selected ? 'sel' : ''}`}>
                    {/* 11-04 07:39 ①: 列内事件片退役——事件归下方 24h 轴（19:52 语义）,
                        消双重显示+无事件列 180px 灰空框（用户红圈空白）。insts 仅留头部判空。 */}
                    <button className="week-col-head" aria-label={`查看 ${k}`}
                      /* 11-05 12:16 ①: 点日→跳对应日页签。
                         11-09 17:46 用户令(纠偏): 恢复前版(跳日页签)——「点击某个框」指 24h 轴时间格(见 week-days 委托),
                         列头右下加号方案撤销(被观感否决「大蓝点」)。 */
                      onClick={() => { setSelected(k); setMode('day'); setWeekSlot(null); }}>
                      <span className="week-num">{d.getDate()}</span>
                      {/* 11-04 23:31 ②: 周条日期带农历（同月历 lunarSub——用 d 实年月防跨月错）。
                          11-08 22:01 ②: wd 上移后列头两行,农历入 sel 框（mt 10→2 撤「推出框底」）。 */}
                      <span className={`week-lunar${lunarSub(d.getDate(), d.getFullYear(), d.getMonth()).isStat ? ' fest' : ''}`}>{lunarSub(d.getDate(), d.getFullYear(), d.getMonth()).sub}</span>
                    </button>
                  </div>
                );
              })}
              </div>
            </>
          );
        })()}
        {mode === 'year' && (() => {
          /** DK-CALENDAR-04 任务 2 + CALENDAR-VIVO 复刻（10-31 11:33 用户单图再报）:
           * 年视图=12 个真·迷你月历格（月标题 01..12 粗+周日开头星期行+日期数字
           * 网格+事件红下划线+节日红数字+当月标题红）——vivo 规格书级复刻。
           * 点月格→monthOffset 定位+切月视图。 */
          const y2 = view.getFullYear() + yearDelta;
          /* 11-02 11:32 报障修: 干支/生肖随 y2 变（原固定今天——滑年后红带不变）。
             取 y2 年中点定干支: 全年稳定一个生肖年（避开 1-2 月春节跨年歧义——vivo 年页语义=整年一个生肖）;
             农历日部分保持今天（用户今天所处农历日）。 */
          const nowLunar = Lunar.fromDate(new Date());
          const y2Lunar = Lunar.fromDate(new Date(y2, 5, 1));
          return (
            <div
              className="year-vivo" key={y2}
              onPointerDown={(e) => { yearTouch.current = { x: e.clientX, y: e.clientY }; }}
              onPointerUp={(e) => {
                const t = yearTouch.current; if (!t) return;
                const dx = e.clientX - t.x;
                const dy = e.clientY - t.y;
                yearTouch.current = null;
                if (Math.abs(dy) > 64 && Math.abs(dx) < 60) setYearDelta((d) => d + (dy < 0 ? 1 : -1));
              }}
              onPointerCancel={() => { yearTouch.current = null; }}
              onTouchEnd={(e) => {
                /* 双通道兜底: touch-action:none 下 touchend 必达（pointer 流个别 ROM 被吞时保底） */
                const t = yearTouch.current; if (!t) return;
                const tc = e.changedTouches[0]; if (!tc) return;
                const dx = tc.clientX - t.x, dy = tc.clientY - t.y;
                yearTouch.current = null;
                if (Math.abs(dy) > 64 && Math.abs(dx) < 60) setYearDelta((d) => d + (dy < 0 ? 1 : -1));
              }}
            >
              <div className="year-lunar-band">
                — {y2Lunar.getYearInGanZhi()}{y2Lunar.getYearShengXiao()}年 — 农历{nowLunar.getDayInChinese()} —
              </div>
              <div className="year-grid">
                {Array.from({ length: 12 }, (_, i) => {
                  const mFrom = new Date(y2, i, 1);
                  const mTo = new Date(y2, i + 1, 0, 23, 59, 59);
                  const dim = mTo.getDate();
                  const lead = mFrom.getDay(); // 周日开头（vivo 规格书——区别月视图的周一开头）
                  /** 事件日集合（红下划线标）——expandAll 覆盖整月。 */
                  const evDays = new Set<number>();
                  for (const inst of expandAll(events, mFrom, mTo)) {
                    const ds = inst.start instanceof Date ? inst.start : new Date(inst.start);
                    if (ds.getFullYear() === y2 && ds.getMonth() === i) evDays.add(ds.getDate());
                  }
                  const isCur = y2 === now.getFullYear() && i === now.getMonth();
                  return (
                    <button key={i} className={`year-month ${isCur ? 'cur' : ''}`}
                      aria-label={`进入 ${y2} 年 ${i + 1} 月`}
                      onClick={() => {
                        /* 深检 BUG2: selected 旧日脱节——目标月含今选今,否则该月 1 号（聚合/高亮与视图一致）。 */
                        const tInM = y2 === now.getFullYear() && i === now.getMonth();
                        const selKey = tInM ? todayStr : `${y2}-${String(i + 1).padStart(2, '0')}-01`;
                        setMonthOffset((y2 - now.getFullYear()) * 12 + i - now.getMonth());
                        setSelected(selKey);
                        setMode('month');
                      }}>
                      <span className="ym-name">{String(i + 1).padStart(2, '0')}</span>
                      <span className="ym-week">
                        {['日', '一', '二', '三', '四', '五', '六'].map((w) => <i key={w}>{w}</i>)}
                      </span>
                      <span className="ym-dates">
                        {Array.from({ length: lead }, (_, k) => <i key={`e${k}`} />)}
                        {Array.from({ length: dim }, (_, k) => {
                          const d = k + 1;
                          const ls = lunarSub(d, y2, i);
                          const hasEv = evDays.has(d);
                          const isToday = isCur && d === now.getDate();
                          return (
                            <i key={d} className={`${hasEv ? 'ev' : ''} ${ls.isStat ? 'fest' : ''} ${isToday ? 'today' : ''}`}
                            >{d}</i>
                          );
                        })}
                      </span>
                    </button>
                  );
                })}
              </div>
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
        {/* 11-05 12:16 ②③: 周=天列×24h 横轴网格——事件块按列定位;左竖线=周日列左界,时间刻度右对齐贴线。 */}
        {mode === 'week' && (() => {
          const w2 = new Date(`${selected}T00:00:00`);
          w2.setDate(w2.getDate() - w2.getDay());
          const days2 = Array.from({ length: 7 }, (_, i) => {
            const d = new Date(w2); d.setDate(d.getDate() + i); return d;
          });
          const w2End = new Date(w2); w2End.setDate(w2End.getDate() + 7);
          const pad2 = (n: number) => String(n).padStart(2, '0');
          const kOf = (d: Date) => `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())}`;
          const dayIdx = (d: Date) => Math.round((new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime() - w2.getTime()) / 86400000);
          type WBlock = { id: string; key: string; col: number; topMin: number; hMin: number; label: string; ev?: CalEvent; kind: 'task' | 'note' | 'diary' | 'event' };
          const blocks: WBlock[] = [];
          for (const inst of expandAll(events, w2, w2End)) {
            if (inst.ev.all_day) continue;
            const c = dayIdx(inst.start);
            if (c < 0 || c > 6) continue;
            const sMin = inst.start.getHours() * 60 + inst.start.getMinutes();
            const eEnd = inst.ev.end ? new Date(inst.ev.end) : inst.start; /* EvInstance 无 end——取 ev.end 解析 */
            const eMin = Math.min(24 * 60, Math.max(sMin + 15, (eEnd.getHours() * 60 + eEnd.getMinutes()) + (dayIdx(eEnd) !== c ? 24 * 60 : 0)));
            blocks.push({ id: inst.ev.id, key: `we-${inst.ev.id}-${c}`, col: c, topMin: sMin, hMin: eMin - sMin, label: inst.ev.title, ev: inst.ev, kind: 'event' });
          }
          for (const t of tasks) {
            if (!t.due) continue;
            const d = new Date(t.due);
            const c = dayIdx(d);
            if (c < 0 || c > 6) continue;
            const sMin = d.getHours() * 60 + d.getMinutes();
            blocks.push({ id: t.id, key: `wt-${t.id}`, col: c, topMin: sMin, hMin: 30, label: t.text, kind: 'task' });
          }
          const todayCol = days2.findIndex((d) => kOf(d) === todayStr);
          const nowH = now.getHours(), nowM = now.getMinutes();
          /* 11-08 22:19 用户令: 周中日程块显字——≥30min 块（高≥20px 容 1 行字）且同列无重叠→文字;
             小块/同列时间重叠互相盖住的→色点（.we-dot,absolute 居中不受块高/padding 裁剪）。 */
          const overlapped = new Set<string>();
          for (let c = 0; c < 7; c++) {
            const arr = blocks.filter((b) => b.col === c).sort((a, b) => a.topMin - b.topMin);
            for (let i = 0; i < arr.length; i++)
              for (let j = i + 1; j < arr.length; j++) {
                if (arr[j].topMin < arr[i].topMin + arr[i].hMin) { overlapped.add(arr[i].key); overlapped.add(arr[j].key); }
                else break;
              }
          }
          const showText = (b: WBlock) => b.hMin >= 30 && !overlapped.has(b.key);
          /* 11-06 12:19 用户三令: ①0 点之上/24 点之下各留一格线,刻度增 24 点——顶空 48+0..23+24 字(心=行顶线);
             ②now 线提层到 week-days 贯整周 7 列(原仅当天列);③右端小三角。事件/线 y 统一 +48 顶空偏移。 */
          return (
            <div className="week-axis" ref={weekAxisRef}>
              <div className="week-times">
                <span className="wt-pad" aria-hidden="true" />
                {Array.from({ length: 24 }, (_, h) => (
                  <span key={h}><i>{String(h)}</i></span>
                ))}
                <span className="wt-24"><i>24</i></span>
              </div>
              <div className="week-days"
                /* 11-09 17:46 用户令④(纠偏): 点灰色日程区时间格(横时间线×纵日期列交织格)→选中该格
                   (格蓝框+加号,加号→快建该日该时段)。事件委托: 列=closest(.week-day-col).data-col,
                   行=(clientY-容器顶-48)/48(顶空 48+行高 48);事件块点击 stopPropagation 不选格。 */
                onClick={(e) => {
                  const colEl = (e.target as HTMLElement).closest('.week-day-col');
                  if (!colEl || !colEl.dataset.col) return;
                  const rect = e.currentTarget.getBoundingClientRect();
                  const h = Math.floor((e.clientY - rect.top - 48) / 48);
                  if (h < 0 || h > 23) return;
                  const c = Number(colEl.dataset.col);
                  setWeekSlot(weekSlot && weekSlot.col === c && weekSlot.h === h ? null : { col: c, h });
                }}>
                {/* 11-08 08:41: now 三角 SVG 矢量版——border 三角 DPR3 光栅量化(0.1px 调整失效)根除;
                    位移走 --tri-dy(CSS 变量,JS 按 devicePixelRatio 反算物理px对齐,全 DPR 渲染一致)。
                    viewBox 5×6 同旧 border 5×6 视觉;理论初值三角身[-2,+4]心=线心(border-top 2px 心=1)。 */}
                <span className="week-now" aria-hidden="true" style={{ top: `${(48 + ((nowH * 60 + nowM) * 48) / 60).toFixed(1)}px` }}>
                  <svg className="now-tri" width="5" height="6" viewBox="0 0 5 6" aria-hidden="true">
                    <path d="M0 0 L5 3 L0 6 Z" fill="var(--cal-accent)" />
                  </svg>
                  {/* 11-09 15:39 用户令: 周页 now 线也挂时间标签（同日页,左侧刻度列骑线）。 */}
                  <i className="week-now-time">{String(nowH).padStart(2, '0')}:{String(nowM).padStart(2, '0')}</i>
                </span>
                {/* 11-09 17:46 ④: 选中时间格蓝框+加号（同日页 slot-sel 观感体系）;加号→快建(列日+格时段)。 */}
                {weekSlot && (
                  <div className="week-slot-sel" aria-hidden="true"
                    style={{ left: `${(weekSlot.col * 100) / 7}%`, width: `${100 / 7}%`, top: `${48 + weekSlot.h * 48}px`, height: '48px' }}>
                    <button type="button" className="slot-plus week-slot-plus" aria-label={`在 ${days2[weekSlot.col] ? kOf(days2[weekSlot.col]) : ''} ${String(weekSlot.h).padStart(2, '0')}:00 新建日程`}
                      onClick={(e) => { e.stopPropagation(); setQuickDay(kOf(days2[weekSlot.col])); setQuickNew(weekSlot.h); setQuickTitle(''); }}>+</button>
                  </div>
                )}
                {days2.map((d, i) => (
                  <div key={kOf(d)} className="week-day-col" data-col={i}>
                    {blocks.filter((b) => b.col === i).map((b) => (
                      <button type="button" key={b.key} className={`week-ev ${b.kind}${showText(b) ? '' : ' dot'}`} style={{ top: `${48 + (b.topMin * 48) / 60}px`, height: `${(b.hMin * 48) / 60 - 4}px` }}
                        aria-label={b.label}
                        onClick={(e) => { e.stopPropagation(); if (b.ev) setEventEditor(b.ev); else if (b.kind === 'task') onNewTask(kOf(d)); }}>
                        {/* 11-08 22:19: 块放不下字（<30min 或同列重叠）→色点;正常→标题。 */}
                        {showText(b) ? b.label : <i className="we-dot" aria-hidden="true" />}
                      </button>
                    ))}
                  </div>
                ))}
              </div>
            </div>
          );
        })()}

        {/* 11-03 19:52 ④: 周页签下方=选中日 24 小时任务轴（同日页——去宜忌聚合）。 */}
        {mode === 'day' && (
          <div className="day-axis" key={'axis-' + selected} aria-label={`日轴 ${selected}`}>
          {/* 11-04 09:4x 多周 BUG 根修: 原裸 key={selected} 与兄弟 week-grid key={dKey(wStart)} 在 selected=周一时撞串——React 同 key reconcile 错乱致 DOM 错挂+多份 week-grid 残留（真机「多个周显示」）。前缀消撞。 */}
            {/* 全天区：任务（due 无时刻）+ 全天事件 */}
            {(() => {
              const items = dayAxis.get(-1) ?? [];
              if (items.length === 0) return null;
              return (
                <div className="day-axis-row all-day">
                  <span className="day-axis-time"><i>全天</i></span>
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
            {/* 11-04 19:20 ①: 24h 全时段轴（原 08-22）——容器可滚（css max-height）;nowline 全天窗 /1440。 */}
            <div className="day-axis-hours" ref={axisHoursRef} data-selected={selected}>
              {/* 11-09 07:31 用户令: 红线常显——原 selected===todayStr 才渲染,跳转/选非今日后线消失。
                 红线=当前时刻参照（同周页贯 7 列语义）,与 selected 哪天无关。 */}
              <span className="day-now-line" aria-hidden="true"
                /* 11-06 12:19: 顶空格 +48 后线位（原 23:37 纯公式,现 y 全域平移）;11-08 08:41 三角 SVG 矢量版(同周页,--tri-dy DPR 反算)。 */
                style={{ top: `${(48 + (nowMin * 48) / 60).toFixed(1)}px` }}>
                <svg className="now-tri" width="5" height="6" viewBox="0 0 5 6" aria-hidden="true">
                  <path d="M0 0 L5 3 L0 6 Z" fill="var(--cal-accent)" />
                </svg>
                <i className="day-now-time">{String(Math.floor(nowMin / 60)).padStart(2, '0')}:{String(nowMin % 60).padStart(2, '0')}</i>
              </span>
              {/* 11-06 12:19 ①: 0 点之上留一格——空 row 占位。 */}
              <div className="day-axis-row day-pad" aria-hidden="true" />
              {Array.from({ length: 24 }, (_, i) => i).map((h) => {
              const items = dayAxis.get(h) ?? [];
              return (
                <div key={h} className={`day-axis-row${slotSel === h ? ' slot-sel' : ''}`}
                  onClick={() => setSlotSlot(slotSel === h ? null : h)}>
                  <span className="day-axis-time"><i>{String(h)}</i></span>
                  {slotSel === h && (
                    <button type="button" className="slot-plus" aria-label={`在 ${String(h).padStart(2, '0')}:00 新建日程`}
                      onClick={(e) => { e.stopPropagation(); setQuickNew(h); setQuickTitle(''); }}>+</button>
                  )}
                  <div className="day-axis-items">
                    {items.map((it) => it.kind === 'event' && it.ev ? (
                      <span key={`${it.kind}-${it.id}-${h}`} className="day-axis-item event" role="button" style={{
                        background: `rgba(${parseInt(EV_COLORS[it.ev.color % 6].slice(1, 3), 16)}, ${parseInt(EV_COLORS[it.ev.color % 6].slice(3, 5), 16)}, ${parseInt(EV_COLORS[it.ev.color % 6].slice(5, 7), 16)}, 0.15)`,
                        color: EV_COLORS[it.ev.color % 6],
                      }} onClick={(e) => { e.stopPropagation(); setEventEditor(it.ev!); }}>{it.label}</span>
                    ) : (
                      <span key={`${it.kind}-${it.id}-${h}`} className={`day-axis-item ${it.kind}`} role="button"
                        onClick={(e) => { e.stopPropagation(); onOpenNote(it.id, it.label); }}>{it.label}</span>
                    ))}
                  </div>
                </div>
              );
              })}
              {/* 11-06 12:19 ①: 24 点刻度——心=23 行底线（=h×48 顶线基）;其下留一格线。 */}
              <div className="day-axis-row day-24"><span className="day-axis-time"><i>24</i></span></div>
            </div>
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
      </div>

      {/* 当日聚合（黄历宜忌+空态提示+FAB 引导）: 11-03 19:52 ④ 周日页签改 24h 任务轴——聚合区限月页。
          21:40 ①: 年视图不显示（年页仅纯 12 月格）。 */}
      {mode === 'month' && (<>
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

      {/* 深检 BUG1: 原 (B||C)===false 对 number 恒 false（严格等 0===false 不成立）——空日提示死码,复活。 */}
      {isSelInMonth && selData.dayTasks.length === 0 && selData.dayNotes.length === 0 && selData.dayDiary.length === 0 && selEvents.length === 0 && (
        <EmptyState text="这一天没有任务或笔记记录。点右下角 + 捕获。" />
      )}

      {/* 深检 BUG4: 月页聚合区从未渲染事件（selEvents 只进日轴）——选中含日程日下方看不到任何日程。
          补事件卡（色点+标题+时刻,点击开编辑）;含事件的日不再误显空态。 */}
      {selEvents.length > 0 && (
        <div className="card" style={{ padding: 4, marginBottom: 8 }}>
          {selEvents.map((inst) => (
            <div key={`${inst.ev.id}@${inst.offsetDays}`} className="task-item" role="button"
              style={{ cursor: 'pointer' }} onClick={() => setEventEditor(inst.ev)}>
              <span className="day-axis-item event"
                style={{ background: `rgba(${parseInt(EV_COLORS[inst.ev.color % 6].slice(1, 3), 16)}, ${parseInt(EV_COLORS[inst.ev.color % 6].slice(3, 5), 16)}, ${parseInt(EV_COLORS[inst.ev.color % 6].slice(5, 7), 16)}, 0.15)`, color: EV_COLORS[inst.ev.color % 6] }}>
                {inst.ev.all_day ? '全天' : `${String(inst.start.getHours()).padStart(2, '0')}:${String(inst.start.getMinutes()).padStart(2, '0')}`}
              </span>
              <div className="task-body">
                <div className="task-text">{inst.ev.title}</div>
                {inst.ev.location && <div className="task-meta"><span>{inst.ev.location}</span></div>}
              </div>
            </div>
          ))}
        </div>
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
      </>
      )}

      {/* CALENDAR-VIVO 11-01 20:39 用户修订: FAB 与其他页同位（16/72）+**仅日程**
          （任务菜单已拆——FAB 直开 EventEditorSheet 选中日 09:00 预填）;sheet 开时隐藏。 */}
      {/* 「今」透明圆: 加号上方并排（年模式加号藏→solo 落原加号位）;sheet 开时隐藏。 */}
      {offToday && !eventEditor && (
        <button
          className={'fab-today' + (mode === 'year' ? ' fab-today-solo' : '')}
          onClick={goToday}
          aria-label="回到今天"
        >今</button>
      )}

      {/* 11-02 07:12: 年模式隐藏 FAB（浏览年语境无创建入口——v27.3 双入口在月/日语境）;sheet 开时隐藏依旧。 */}
      {!eventEditor && mode !== 'year' && (
        <button
          className="fab fab-cal"
          onClick={() => setEventEditor({
            id: '', title: '',
            /* 11-08 22:19 用户令: 新建默认起止=当前时刻（原固定 09:00/end 空）——日期=selected 日,钟点=当前。 */
            start: `${isSelInMonth ? selected : `${y}-${String(m + 1).padStart(2, '0')}-01`}T${String(nowMin / 60 | 0).padStart(2, '0')}:${String(nowMin % 60).padStart(2, '0')}:00`,
            end: `${isSelInMonth ? selected : `${y}-${String(m + 1).padStart(2, '0')}-01`}T${String(nowMin / 60 | 0).padStart(2, '0')}:${String(nowMin % 60).padStart(2, '0')}:00`,
            all_day: false, location: null, note_id: null, color: 1,
          })}
          aria-label={`为 ${selected} 添加日程`}
        >{I.plus}</button>
      )}

      {/* 11-05 08:40 ③: 时段加号→简要版快建（更多设置→完整版 prefilled）。 */}
      {quickNew !== null && !eventEditor && (() => {
        const q = dayIso(quickNew);
        return (
          <>
            <div className="sheet-mask" onClick={() => closeQuick()} />
            <div className="sheet quick-sheet" role="dialog" aria-label="快捷新建日程">
              <div className="sheet-grab" />
              <div className="sheet-head">
                <span className="sheet-title">新建日程</span>
                <button className="icon-btn" onClick={() => closeQuick()} aria-label="关闭">{I.x}</button>
              </div>
              <div className="sheet-body">
                <input className="input" placeholder="准备做什么？" value={quickTitle}
                  onChange={(e) => setQuickTitle(e.target.value)} aria-label="日程标题" autoFocus />
                <div className="quick-time-row">
                  <span className="quick-time">{quickNew === 23 ? `${q.mLabel} 23:00 - 次日 00:00` : `${q.mLabel} ${String(quickNew).padStart(2, '0')}:00 - ${String(quickNew + 1).padStart(2, '0')}:00`}</span>
                  <button type="button" className="quick-more" onClick={() => {
                    /* 跳完整版: 关快建→开编辑器（起止 prefilled 本时段）。 */
                    closeQuick();
                    setEventEditor({ id: '', title: quickTitle, start: q.start, end: q.end, all_day: false, location: null, note_id: null, color: 0, repeat: null, reminders: null });
                  }}>更多设置</button>
                </div>
                <button type="button" className="quick-done" disabled={!quickTitle.trim()}
                  onClick={() => {
                    onUpsertEvent({ id: '', title: quickTitle.trim(), start: q.start, end: q.end, all_day: false, location: null, note_id: null, color: 0, repeat: null, reminders: null });
                    closeQuick(); setSlotSel(null); setWeekSlot(null);
                  }}>完成</button>
              </div>
            </div>
          </>
        );
      })()}

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
  const [delConfirm, setDelConfirm] = useState(false);
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
      {/* ③ 11-01 20:22 用户指令: 日程编辑页全屏 */}
      <div className="sheet sheet-full">
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
              {/* 11-04 21:45 ①: 「分钟回车」→「添加」按钮（从 label 内取输入框,回车快捷保留）。 */}
              <button type="button" className="ev-rem-add" aria-label="添加自定义提醒"
                onClick={(e) => {
                  const inp = (e.currentTarget.closest('.ev-interval') as HTMLElement)?.querySelector('input');
                  if (inp) { const v = parseInt((inp as HTMLInputElement).value, 10); if (!Number.isNaN(v) && v >= 0) { toggleReminder(v); (inp as HTMLInputElement).value = ''; } }
                }}>＋ 添加</button>
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
              /* 11-04 21:45 ②: window.confirm 在 Android WebView 无 WebChromeClient 时静默返 false
                 =删除零反应。改两段式: 首点变「确认删除」3s 窗口,再点真删。 */
              onClick={() => {
                if (delConfirm) { onDelete(ev.id); setDelConfirm(false); return; }
                setDelConfirm(true);
                window.setTimeout(() => setDelConfirm(false), 3000);
              }}>
              {delConfirm ? '再点一次确认删除' : '删除日程'}
            </button>
          )}
        </div>
      </div>
    </>
  );
}
