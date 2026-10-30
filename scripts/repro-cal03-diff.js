/**
 * DK-CALENDAR-03 对拍深挖 — TS expandEvent vs Rust repeat_engine::expand
 * 六用例语义对拍（各时域同墙钟锚→墙钟序列一致;中国无 DST,对拍合法）。
 * Rust 期望值逐字取自 repeat_engine.rs 单测①-⑧。
 */
const { expandEvent } = require('/tmp/repeatUtil.cjs');

const D = (s) => new Date(s);
const fmt = (d) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}T${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;

const ev = (start, repeat) => ({
  id: 'e', title: 't', start, end: null, all_day: false,
  location: null, note_id: null, color: 0, repeat, created_at: start, updated_at: start,
});

let pass = 0, fail = 0;
const check = (name, got, want) => {
  const g = got.map((x) => fmt(x.start)).join(' | ');
  const w = want.join(' | ');
  const ok = g === w;
  ok ? pass++ : fail++;
  console.log(`${ok ? 'PASS' : 'FAIL'} ${name}\n  got : ${g}\n  want: ${w}`);
};

// ① weekly 每周五×3 周（锚 10-30 周五 09:00;to=11-13 23:59——11-20 不在窗）
check('① weekly 3 周',
  expandEvent(ev('2026-10-30T09:00:00', { freq: 'weekly', interval: 1, until: null, by_weekday: null, day_of_month: null, count: null, lunar_month: null, lunar_day: null }), D('2026-10-30T09:00'), D('2026-11-13T23:59')),
  ['2026-10-30T09:00', '2026-11-06T09:00', '2026-11-13T09:00']);

// ② by_weekday 周一+周四（锚 10-26 周一 08:00;to=11-02 深夜）
check('② by_weekday 多日',
  expandEvent(ev('2026-10-26T08:00:00', { freq: 'weekly', interval: 1, until: null, by_weekday: [0, 3], day_of_month: null, count: null, lunar_month: null, lunar_day: null }), D('2026-10-26T08:00'), D('2026-11-02T23:59')),
  ['2026-10-26T08:00', '2026-10-29T08:00', '2026-11-02T08:00']);

// ③ monthly 31→短月钳（锚 2026-01-31 10:00）
check('③ 月末 31 钳制',
  expandEvent(ev('2026-01-31T10:00:00', { freq: 'monthly', interval: 1, until: null, by_weekday: null, day_of_month: null, count: null, lunar_month: null, lunar_day: null }), D('2026-01-31T10:00'), D('2026-03-31T23:59')),
  ['2026-01-31T10:00', '2026-02-28T10:00', '2026-03-31T10:00']);

// ④ yearly 2-29→非闰 2-28（锚 2024-02-29 12:00）
check('④ 闰年 2-29 钳制',
  expandEvent(ev('2024-02-29T12:00:00', { freq: 'yearly', interval: 1, until: null, by_weekday: null, day_of_month: null, count: null, lunar_month: null, lunar_day: null }), D('2024-02-29T12:00'), D('2026-12-31T23:59')),
  ['2024-02-29T12:00', '2025-02-28T12:00', '2026-02-28T12:00']);

// ⑤ count 截断含范围前消耗（daily 锚 10-01 09:00 count=3;from 10-03）
check('⑤ count 截断',
  expandEvent(ev('2026-10-01T09:00:00', { freq: 'daily', interval: 1, until: null, by_weekday: null, day_of_month: null, count: 3, lunar_month: null, lunar_day: null }), D('2026-10-03T00:00'), D('2026-10-31T23:59')),
  ['2026-10-03T09:00']);

// ⑥ until 截断（daily 锚 10-01 09:00 until=10-03 09:00;from=锚）
check('⑥ until 截断',
  expandEvent(ev('2026-10-01T09:00:00', { freq: 'daily', interval: 1, until: '2026-10-03T09:00:00.000Z', by_weekday: null, day_of_month: null, count: null, lunar_month: null, lunar_day: null }), D('2026-10-01T09:00'), D('2026-10-31T23:59')),
  ['2026-10-01T09:00', '2026-10-02T09:00', '2026-10-03T09:00']);

// ⑦ 农历生日（锚 2024-02-10=正月初一;2025→01-29,2026→02-17——Playwright 6/6 用例移植 node 面）
check('⑦ 农历年反推',
  expandEvent(ev('2024-02-10T09:00:00', { freq: 'yearly', interval: 1, until: null, by_weekday: null, day_of_month: null, count: null, lunar_month: 1, lunar_day: 1 }), D('2025-01-01T00:00'), D('2026-12-31T23:59')),
  ['2025-01-29T09:00', '2026-02-17T09:00']);

// ⑧ weekly interval=2（锚 10-30 周五;每 2 周→11-13/11-27）
check('⑧ 间隔 2 周',
  expandEvent(ev('2026-10-30T09:00:00', { freq: 'weekly', interval: 2, until: null, by_weekday: null, day_of_month: null, count: null, lunar_month: null, lunar_day: null }), D('2026-10-30T09:00'), D('2026-11-27T23:59')),
  ['2026-10-30T09:00', '2026-11-13T09:00', '2026-11-27T09:00']);

console.log(`\n== CAL-03 对拍: ${pass}/${pass + fail} PASS ==`);
process.exit(fail > 0 ? 1 : 0);
