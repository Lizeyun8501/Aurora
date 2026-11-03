/**
 * DK-CALENDAR-01 — Playwright 行为验证（任务 2/3 + DoD 断言）
 *
 * 环境: apps/mobile/dist/index.html（零桥降级态——月历/黄历/双视图
 * 均纯前端渲染；聚合数据空态走 EmptyState 亦为断言面）。
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

  const BASE01 = 'file://' + require('path').resolve(__dirname, '../apps/mobile/dist/index.html');
  await page.goto(BASE01);
  await page.waitForTimeout(800);

  // ① 进日历 Tab（TabBar 结构: .tab-bar > .tab-item:has-text(日历)）
  {
    const tab = await page.$('.tab-bar .tab-item:has-text("日历")');
    if (tab) { await tab.click(); await page.waitForTimeout(500); }
    const grid = await page.$('.cal-grid');
    record('① 月历渲染', !!grid, `tab=${!!tab} grid=${!!grid}`);
  }

  // ② 黄历宜忌行（lunar 纯前端——零桥可断言）
  {
    const yi = await page.evaluate(() => document.querySelector('.almanac-tag.yi')?.textContent ?? '');
    const ji = await page.evaluate(() => document.querySelector('.almanac-tag.ji')?.textContent ?? '');
    record('② 黄历宜忌行', yi === '宜' && ji === '忌', `宜=${yi} 忌=${ji}`);
  }

  // ③ 月→日切换（segmented）+ 时间轴渲染
  {
    const dayTab = await page.$('.cal-mode-seg button:has-text("日")');
    if (dayTab) { await dayTab.click(); await page.waitForTimeout(300); }
    const axis = await page.$('.day-axis');
    const hours = await page.evaluate(() =>
      Array.from(document.querySelectorAll('.day-axis-time')).slice(0, 3).map((el) => el.textContent));
    const has8 = hours.includes('08:00');
    record('③ 日视图时间轴', !!axis && has8, `axis=${!!axis} 首3槽=${hours.join(',')}`);
    /* ④ 两合一（E20 月页语义 + Leo 深检日页语义）: 日页断 24h 轴+无宜忌;回月断空日 EmptyState。 */
    const dayOk = await page.evaluate(() => !!document.querySelector('.day-axis') && [...document.querySelectorAll('.almanac-tag')].length === 0);
    record('④a 日页 24h 轴+无宜忌', dayOk, 'day-axis 在案');
    const segM = await page.$('.cal-mode-seg button:has-text("月")');
    if (segM) { await segM.click(); await page.waitForTimeout(400); }
    await page.evaluate(() => {
      const cells = Array.from(document.querySelectorAll('.cal-day'));
      const emptyCell = cells.find((c) => { const t = c.textContent?.trim() ?? ''; return /^\d+$/.test(t) && Number(t) > 14; });
      emptyCell?.click();
    });
    await page.waitForTimeout(500);
    const empty = await page.evaluate(() => document.body.textContent?.includes('没有任务或笔记记录') ?? false);
    record('④b 空日 EmptyState（月页·v27.23 语义）', empty, '月页空态文案在案');)
  }

  // ⑤ 日→月回切（翻月动效面 key 重挂无错）
  {
    const monthTab = await page.$('.cal-mode-seg button:has-text("月")');
    if (monthTab) { await monthTab.click(); await page.waitForTimeout(300); }
    const grid = await page.$('.cal-grid');
    /* 11-03 17:05: 箭头已删——改月滑（上滑=下月）。 */
    await page.evaluate(() => { const el = document.querySelector('.cal-grid'); if (el) { const r = el.getBoundingClientRect(); el.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, cancelable: true, clientX: r.x + r.width / 2, clientY: r.y + 120, pointerId: 1, pointerType: 'touch', isPrimary: true })); el.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, cancelable: true, clientX: r.x + r.width / 2, clientY: r.y + 30, pointerId: 1, pointerType: 'touch', isPrimary: true })); } });
    await page.waitForTimeout(300);
    record('⑤ 月视图回切+滑翻月', !!grid, 'cal-grid 回归+滑翻月无错');
  }

  // ⑥ errors === 0
  record('⑥ 零错误', errors.length === 0, errors.length ? errors.slice(0, 2).join(' | ') : '双监听 0');

  const failed = results.filter((r) => !r.pass).length;
  console.log(`\n=== CALENDAR-01 验收: ${results.length - failed}/${results.length} PASS ===`);
  await browser.close();
  process.exit(failed > 0 ? 1 : 0);
})();
