/** CALENDAR-VIVO 11-01 20:22 三连改断言: ①选中白方块 ②胶囊独立行 ③编辑全屏+FAB 隐藏。 */
const { chromium } = require('/home/z/.npm-global/lib/node_modules/playwright');
(async () => {
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 390, height: 844 }, hasTouch: true });
  const errors = [];
  page.on('pageerror', (e) => errors.push(String(e)));
  await page.goto('file:///home/z/my-project/repos/Aurora/apps/mobile/dist/index.html');
  await page.waitForTimeout(1100);
  await page.click('.tab-item:has-text("日历")'); await page.waitForTimeout(500);
  const fail = []; const ok = (n, c) => { (c ? console.log('  ✓ ' + n) : (console.log('  ✗ ' + n), fail.push(n))); };

  // ① 点非今日日期 → 选中格白底方块（非蓝红双圈）
  await page.click('.cal-day-btn:not(.today) >> nth=10'); await page.waitForTimeout(300);
  const sel = await page.evaluate(() => {
    const el = document.querySelector('.cal-day-btn.sel');
    if (!el) return null;
    const cs = getComputedStyle(el);
    return { bg: cs.backgroundColor, radius: cs.borderRadius, shadow: cs.boxShadow.slice(0, 30) };
  });
  ok(`① 选中=白底方块（bg=${sel?.bg}, r=${sel?.radius}）`, !!sel && /255, 255, 255/.test(sel.bg) && sel.radius === '12px');
  ok(`① 轻投影浮起（${sel?.shadow}）`, !!sel && /rgba\(0, 0, 0, 0\.14\)/.test(sel?.shadow ?? ""));
  const today = await page.evaluate(() => {
    const el = document.querySelector('.cal-day.today, .cal-day-btn.today');
    if (!el) return null;
    return { bg: getComputedStyle(el).backgroundColor, color: getComputedStyle(el.querySelector('.cal-num')).color };
  });
  ok(`① 今日无蓝圆（bg=${today?.bg} 数字红=${today?.color}）`, !!today && /rgba\(0, 0, 0, 0\)|transparent/.test(today.bg) && today.color === 'rgb(255, 77, 79)');

  // ② 胶囊条独立行: 不在 cal-head 内+位于年月行下方
  const seg = await page.evaluate(() => {
    const seg = document.querySelector('.cal-mode-seg');
    const head = document.querySelector('.cal-head');
    if (!seg || !head) return null;
    return { inHead: head.contains(seg), segTop: seg.getBoundingClientRect().top, headBottom: head.getBoundingClientRect().bottom, width: seg.getBoundingClientRect().width };
  });
  ok(`② 胶囊独立行（inHead=${seg?.inHead}, 宽=${Math.round(seg?.width ?? 0)}px 通栏）`, !!seg && !seg.inHead && (seg.width ?? 0) > 200);
  ok(`② 位置=年月行下方（Δ=${Math.round((seg?.segTop ?? 0) - (seg?.headBottom ?? 0))}px）`, !!seg && (seg.segTop - seg.headBottom) > 0);

  // ③ FAB → 日程 → 全屏 sheet+FAB 消失
  await page.click('.fab.fab-cal'); await page.waitForTimeout(300);
  await page.click('.fab-mini-ev'); await page.waitForTimeout(500);
  const fs = await page.evaluate(() => {
    const sh = document.querySelector('.sheet.sheet-full');
    if (!sh) return null;
    const r = sh.getBoundingClientRect();
    return { h: r.height, w: r.width, top: r.top, fabN: document.querySelectorAll('.fab').length };
  });
  ok(`③ 编辑页全屏（h=${Math.round(fs?.h ?? 0)}/844, top=${fs?.top}）`, !!fs && (fs.h ?? 0) >= 830 && fs.top === 0);
  ok(`③ 全屏时 FAB 消失（=${fs?.fabN}）`, fs?.fabN === 0);
  await page.locator('.sheet .icon-btn[aria-label="关闭"]').first().click().catch(() => {}); await page.waitForTimeout(350);
  ok('③ 关闭后 FAB 回归', await page.locator('.fab.fab-cal').count() === 1);

  console.log(`\n=== CALENDAR-VIVO3 验收: ${7 - fail.length}/7 ${fail.length ? 'FAIL: ' + fail.join('|') : 'PASS'}（控制台错 ${errors.length}）===`);
  await page.screenshot({ path: '/tmp/vivo3.png' });
  await browser.close();
  process.exit(fail.length || errors.length ? 1 : 0);
})();
