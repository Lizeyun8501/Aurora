/**
 * A-08: 日历 shared 面——双端单份实现（DoD: rg expandAll 只命中
 * shared 与两端 import）。
 *
 * 内容三块:
 * 1. repeatUtil 全量上移（原 apps/mobile/src/views/repeatUtil.ts 逐字迁移——
 *    DK-CALENDAR-03 重复日程展开,与 Rust core repeat_engine.rs 语义镜像,
 *    语义有单测锁定,禁止改写）
 * 2. lunarSub 农历副标（原 CalendarView 内嵌——上移后桌面 aria 同源）
 * 3. CalendarVm 类型镜像（crates/aurora-core/src/calendar_vm.rs snake_case 同构）
 */
import { Solar, Lunar, HolidayUtil, LunarYear } from 'lunar-typescript';

// ===================================================================
// 类型面
// ===================================================================

/** 日程事件（与 entity::Event serde snake_case 同构——FFI/IPC 直传零转换）。 */
export interface CalEvent {
  id: string;
  title: string;
  start: string;
  end: string | null;
  all_day: boolean;
  location: string | null;
  note_id: string | null;
  color: number;
  /** 重复规则（None=单次）。 */
  repeat?: {
    freq: 'daily' | 'weekly' | 'monthly' | 'yearly';
    interval: number;
    until: string | null;
    by_weekday?: number[] | null;
    day_of_month?: number | null;
    count?: number | null;
    /** 农历生日锚（yearly+此二字段——展开走前端 lunar 反推,与 entity 同构）。 */
    lunar_month?: number | null;
    lunar_day?: number | null;
  } | null;
  reminders?: Array<{ offset_min: number; alarm_level: number }> | null;
  created_at?: string;
  updated_at?: string;
}

/** RepeatRule TS 面（与 entity serde snake_case 同构）。 */
export interface RepeatRuleTs {
  freq: 'daily' | 'weekly' | 'monthly' | 'yearly';
  interval: number;
  until: string | null;
  by_weekday?: number[] | null;
  day_of_month?: number | null;
  count?: number | null;
  lunar_month?: number | null;
  lunar_day?: number | null;
}

/** 展开实例（key = base_id@偏移天数——消费端去重键）。 */
export interface EvInstance {
  ev: CalEvent;
  /** 实例起始时刻（本地 Date——分秒同锚点）。 */
  start: Date;
  /** 与锚点的偏移天数（去重键组成部分）。 */
  offsetDays: number;
}

/** A-08: 日历任务 VM（Rust task_store::CalendarTask snake_case 同构）。
 *  值域对齐移动 GTD: status=next/waiting/plan/done 透传。 */
export interface CalendarTaskVm {
  id: string;
  text: string;
  status: string;
  /** 「YYYY-MM-DD」（undefined=未排期）。 */
  due?: string;
  note_id?: string;
  /** epoch ms。 */
  created_at: number;
  priority?: string;
  /** 移动端扩展字段透传（estimate/actual/parent_task_id/progress 等）。 */
  ext?: unknown;
}

/** A-08: 双端日历统一 VM（Rust build_calendar_vm 输出镜像）。
 *  selectedDate/viewMode=UI 本地态,各端自持（差异披露见 calendar_vm.rs）。 */
export interface CalendarVm {
  events: CalEvent[];
  tasks: CalendarTaskVm[];
}

// ===================================================================
// 重复日程展开（DK-CALENDAR-03——原 mobile repeatUtil.ts 逐字迁移）
// ===================================================================

const LIMIT = 500;
const DAY_MS = 86_400_000;

const pad = (n: number) => String(n).padStart(2, '0');
const dateKey = (d: Date) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
const daysInMonth = (y: number, m: number) => new Date(y, m + 1, 0).getDate();

/** 农历反推: 目标公历年 → 农历(月,日) 的公历日（闰月年无对应闰月→次月同日）。 */
function lunarToSolarYear(y: number, lunarMonth: number, lunarDay: number): Date | null {
  try {
    // Lunar.fromYmd(年,月,日)——月 1-13（闰月为负数惯例本库用 13 表?）
    // lunar-typescript: 闰月表示为负数月（-1=闰正月）——锚月>12 时先查闰月名。
    let m = lunarMonth;
    if (m > 12) {
      // 13=闰X月语义占位: LunarYear.getLeapMonth() 返回闰月序（0=无闰）
      const leap = LunarYear.fromYear(y).getLeapMonth();
      if (leap !== m - 12 && leap !== 0) {
        // 锚是「闰M」但目标年闰的是别的月 → 次月同日（行业惯例）
        m = m - 12 + 1;
      } else if (leap === 0) {
        m = m - 12 + 1; // 目标年无闰月 → 次月同日
      } else {
        m = -leap; // 闰M 命中——库负数表
      }
    }
    const lunar = Lunar.fromYmd(y, m, lunarDay);
    const solar = lunar.getSolar();
    return new Date(solar.getYear(), solar.getMonth() - 1, solar.getDay());
  } catch {
    return null;
  }
}

/** 公历候选生成器（镜像 Rust——yield 锚点起升序时刻）。 */
function* candidates(rule: RepeatRuleTs, anchor: Date): Generator<Date> {
  const interval = Math.max(1, rule.interval);
  const time = { h: anchor.getHours(), m: anchor.getMinutes(), s: anchor.getSeconds() };
  const at = (y: number, mIdx: number, d: number): Date | null => {
    const dim = daysInMonth(y, mIdx);
    const day = Math.min(d, dim); // 短月钳制
    const dd = new Date(y, mIdx, day, time.h, time.m, time.s);
    return Number.isNaN(dd.getTime()) ? null : dd;
  };
  if (rule.freq === 'daily') {
    for (let n = 0; ; n++) {
      const d = new Date(anchor);
      d.setDate(d.getDate() + n * interval);
      yield d;
    }
  } else if (rule.freq === 'weekly') {
    const weekdays = (rule.by_weekday ?? []).length > 0 ? rule.by_weekday! : [anchor.getDay() === 0 ? 6 : anchor.getDay() - 1];
    // 锚点周界（周一）
    const ws = new Date(anchor);
    ws.setDate(ws.getDate() - ((anchor.getDay() + 6) % 7));
    ws.setHours(0, 0, 0, 0);
    for (let n = 0; ; n++) {
      const d = new Date(ws);
      d.setDate(d.getDate() + n);
      const weekIdx = Math.floor((d.getTime() - ws.getTime()) / DAY_MS / 7);
      if (weekIdx % interval === 0) {
        const wd = (d.getDay() + 6) % 7;
        if (weekdays.includes(wd)) yield new Date(d.getFullYear(), d.getMonth(), d.getDate(), time.h, time.m, time.s);
      }
    }
  } else if (rule.freq === 'monthly') {
    const anchorDay = rule.day_of_month ?? anchor.getDate();
    const y0 = anchor.getFullYear();
    const m0 = anchor.getMonth();
    for (let n = 0; ; n++) {
      const total = y0 * 12 + m0 + n * interval;
      const y = Math.floor(total / 12);
      const mIdx = total % 12;
      const d = at(y, mIdx, anchorDay);
      if (d) yield d;
    }
  } else {
    // yearly 公历（农历在前置分流）
    const y0 = anchor.getFullYear();
    for (let n = 0; ; n++) {
      const y = y0 + n * interval;
      const d = at(y, anchor.getMonth(), anchor.getDate());
      if (d) yield d;
    }
  }
}

/** 展开单事件 [from, to] 实例（农历模式走反推;上限 500）。 */
export function expandEvent(ev: CalEvent, from: Date, to: Date): EvInstance[] {
  const rule = ev.repeat as RepeatRuleTs | null | undefined;
  const anchor = new Date(ev.start);
  if (!rule || Number.isNaN(anchor.getTime())) return [];
  const out: EvInstance[] = [];
  const until = rule.until ? new Date(rule.until) : null;

  // 农历生日: yearly + lunar 锚（Rust 返回空的前端对位面）
  if (rule.freq === 'yearly' && rule.lunar_month != null && rule.lunar_day != null) {
    const lm = rule.lunar_month;
    const ld = rule.lunar_day;
    for (let y = anchor.getFullYear(); y <= to.getFullYear() + 1 && out.length < LIMIT; y++) {
      if (y < from.getFullYear() - 1) continue;
      const d = lunarToSolarYear(y, lm, ld);
      if (!d) continue;
      d.setHours(anchor.getHours(), anchor.getMinutes(), anchor.getSeconds());
      let countConsumed = y - anchor.getFullYear();
      if (rule.count != null && countConsumed + 1 > rule.count) break;
      if (until && d > until) break;
      if (d >= from && d <= to) out.push({ ev, start: d, offsetDays: Math.round((d.getTime() - anchor.getTime()) / DAY_MS) });
    }
    return out;
  }

  // 公历（count 消耗计数从锚点起——含范围前已发生）
  let occurrences = 0;
  for (const cand of candidates(rule, anchor)) {
    occurrences++;
    if (occurrences > 10_000) break; // 迭代硬顶
    if (rule.count != null && occurrences > rule.count) break;
    if (cand > to) break;
    if (until && cand > until) break;
    if (cand >= from && out.length < LIMIT) {
      out.push({ ev, start: cand, offsetDays: Math.round((cand.getTime() - anchor.getTime()) / DAY_MS) });
    }
  }
  return out;
}

/** 事件列表 [from, to] 全展开（含非重复事件直传——消费端统一实例面）。 */
export function expandAll(events: CalEvent[], from: Date, to: Date): EvInstance[] {
  const out: EvInstance[] = [];
  for (const ev of events) {
    const start = new Date(ev.start);
    if (ev.repeat) {
      out.push(...expandEvent(ev, from, to));
    } else {
      if (!Number.isNaN(start.getTime()) && start >= from && start <= to) {
        out.push({ ev, start, offsetDays: 0 });
      }
    }
  }
  return out;
}

/** 实例日期键（日格/轴消费口径——本地时区纪律 Leo 8f82772）。 */
export const instanceKey = (inst: EvInstance) => `${inst.ev.id}@${inst.offsetDays}`;
export const instanceDayKey = (inst: EvInstance) => dateKey(inst.start);

// ===================================================================
// 农历副标（原 CalendarView lunarSub 上移——桌面 aria 同源）
// ===================================================================

/** 格子副标: 法定假日名 > 农历节日 > 公历节日 > 节气 > 初一显月名 > 农历日。 */
export function lunarSub(d: number, yy: number, mm: number): { sub: string; isStat: boolean; isWork: boolean } {
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
}

/** 日历格完整 aria-label（A-08 DoD: 含农历信息）。 */
export function calendarCellAria(d: number, yy: number, mm: number, eventCount: number, taskCount: number): string {
  const ls = lunarSub(d, yy, mm);
  const wd = ['日', '一', '二', '三', '四', '五', '六'][new Date(yy, mm, d).getDay()];
  const parts = [`${yy}年${mm + 1}月${d}日 星期${wd}`, ls.sub];
  if (ls.isStat) parts.push('法定假日');
  if (ls.isWork) parts.push('调休上班');
  if (eventCount > 0) parts.push(`${eventCount} 个日程`);
  if (taskCount > 0) parts.push(`${taskCount} 个待办`);
  return parts.join(', ');
}
