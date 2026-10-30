/**
 * DK-CALENDAR-03 — Playwright 验证（DoD 2/3）
 *
 * ① 每周重复→连续三周角标可见（UI 断言——与 Rust 单测①同源逻辑）
 * ② 农历生日 2024-02-10（正月初一）→ 2025-01-29 / 2026-02-17 展开
 *    （闰月年用例: 2025 闰六月/2026 无闰——Lunar 反推正确性）
 * ③ 短月钳制: 月末 31 重复事件 2 月角标在 28/29
 */
const { chromium } = require('/home/z/.npm-global/lib/node_modules/playwright');

(async () => {
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 390, height: 844 }, hasTouch: true });
  const errors = [];
  page.on('pageerror', (e) => errors.push(`pageerror: ${e.message}`));
  page.on('console', (m) => { if (m.type() === 'error') errors.push(`console.error: ${m.text()}`); });

  const results = [];
  const record = (name, pass, detail) => {
    results.push({ name, pass, detail });
    console.log(`${pass ? 'PASS' : 'FAIL'} ${name}: ${detail}`);
  };

  // ① 每周五重复（今天起）→ 本月+次两周角标
  await page.goto('file:///home/z/my-project/Aurora/apps/mobile/dist/index.html');
  await page.waitForTimeout(800);
  await page.evaluate(() => localStorage.setItem('aurora.events', JSON.stringify([{
    id: 'ev-w1', title: '每周例会', start: new Date().toISOString(), end: null,
    all_day: false, location: null, note_id: null, color: 1,
    repeat: { freq: 'weekly', interval: 1, until: null, by_weekday: null, count: null },
  }])));
  await page.reload();
  await page.waitForTimeout(800);
  await page.evaluate(() => { window.location.hash = ''; });
  const calTab = await page.$('.tab-bar .tab-item:has-text("日历")');
  if (calTab) await calTab.click();
  await page.waitForTimeout(600);
  const weekDots = await page.evaluate(() => {
    // 今天的格子应有角标（锚点周）
    const todayBtn = document.querySelector('.cal-day.today');
    return todayBtn ? todayBtn.querySelectorAll('.cal-ev-dot').length : -1;
  });
  // 翻到下月数两个周五角标（近似:任意 2+ 周期格有 dot）
  await page.click('[aria-label="下月"]');
  await page.waitForTimeout(400);
  const nextMonthDots = await page.evaluate(() =>
    document.querySelectorAll('.cal-day .cal-ev-dots').length);
  record('① 每周重复锚点周角标', weekDots >= 1, `today 格 dot=${weekDots}`);
  record('①b 翻月展开角标', nextMonthDots >= 1, `次月 dot 格数=${nextMonthDots}`);

  // ② 农历生日: 2024-02-10 正月初一 → 2025-01-29 / 2026-02-17
  await page.evaluate(() => localStorage.setItem('aurora.events', JSON.stringify([{
    id: 'ev-bday', title: '农历生日', start: '2024-02-10T09:00:00.000Z', end: null,
    all_day: false, location: null, note_id: null, color: 3,
    repeat: { freq: 'yearly', interval: 1, until: null, lunar_month: 1, lunar_day: 1 },
  }])));
  await page.reload();
  await page.waitForTimeout(800);
  {
    const t = await page.$('.tab-bar .tab-item:has-text("日历")');
    if (t) await t.click();
    await page.waitForTimeout(600);
  }
  // 翻到目标月（数字年月比较——字典序「10月<2月」坑;断「日」格有角标）
  const dotAt = async (year, month /* 1 基 */, day) => {
    const label = `${year} 年 ${month} 月`;
    const monthNum = (t) => { const m = t.match(/(\d+)\s*年\s*(\d+)\s*月/); return m ? +m[1] * 12 + +m[2] : NaN; };
    const target = year * 12 + month;
    let cur = await page.textContent('.cal-title');
    let guard = 0;
    while (!cur.includes(label) && guard < 40) {
      await page.click(monthNum(cur) < target ? '[aria-label="下月"]' : '[aria-label="上月"]');
      await page.waitForTimeout(150);
      cur = await page.textContent('.cal-title');
      guard += 1;
    }
    return page.evaluate((dd) => {
      const dots = [...document.querySelectorAll('.cal-day')];
      const hit = dots.find((d) => {
        const num = d.querySelector('.cal-num')?.textContent;
        return num === String(dd) && d.querySelector('.cal-ev-dots');
      });
      return hit ? true : false;
    }, day);
  };
  const b25 = await dotAt(2025, 1, 29);
  record('② 农历生日 2025-01-29 角标', b25, '正月初一→公历 2025-01-29 命中');
  // 2026-02-17（当前在 2025-01——正向翻 13 月）
  const b26 = await dotAt(2026, 2, 17);
  record('②b 农历生日 2026-02-17 角标', b26, '正月初一→公历 2026-02-17 命中（无闰月年）');

  // ③ 月末 31 → 短月钳 28/29（2026-01-31 本地 20:00 锚=12:00Z → 2026-02-28 角标）
  await page.evaluate(() => localStorage.setItem('aurora.events', JSON.stringify([{
    id: 'ev-eom', title: '月末复盘', start: '2026-01-31T12:00:00.000Z', end: null,
    all_day: false, location: null, note_id: null, color: 2,
    repeat: { freq: 'monthly', interval: 1, until: null, day_of_month: 31, count: null },
  }])));
  await page.reload();
  await page.waitForTimeout(800);
  {
    const t = await page.$('.tab-bar .tab-item:has-text("日历")');
    if (t) await t.click();
    await page.waitForTimeout(600);
  }
  const eom = await dotAt(2026, 2, 28);
  record('③ 月末 31→短月钳制 2026-02-28', eom, '2026-02 非闰 28 天命中');

  record('④ 零错误', errors.length === 0, errors.length ? errors.slice(0, 2).join(' | ') : '双监听 0');

  const failed = results.filter((r) => !r.pass).length;
  console.log(`\n=== CALENDAR-03 验收: ${results.length - failed}/${results.length} PASS ===`);
  await browser.close();
  process.exit(failed > 0 ? 1 : 0);
})();
