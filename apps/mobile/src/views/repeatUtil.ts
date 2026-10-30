/**
 * DK-CALENDAR-03 — 重复日程展开（TS 消费侧）
 *
 * 与 Rust core `repeat_engine.rs` 语义镜像（公历子集）+ 农历生日反推
 * （lunar-typescript——Rust 侧农历返回空集的前端对位面）。
 *
 * 架构分工（开工令 2026-10-29）:
 * - 存储层（Rust core event_store）: RepeatRule 序列化往返（snake_case）
 * - 展开层: 公历 Rust/TS 双实现（同语义单测锁定）;农历仅 TS（库现成）
 *
 * 行业惯例（与 Rust 一致——单测锁定）:
 * - 月末 31 → 短月钳月末;2-29 → 非闰年 2-28
 * - count = 锚点起总次数（含范围前;锚点=第 1 次）;until/count 先到为准
 * - 闰月策略: 目标年无对应闰月 → 次月同日（文档化）
 * - 单次展开 ≤ 500 条
 */
import { Solar, Lunar, LunarYear } from 'lunar-typescript';
import type { CalEvent } from '../adapters/androidPlatform';

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
