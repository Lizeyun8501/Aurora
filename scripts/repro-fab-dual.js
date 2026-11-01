/** CALENDAR-VIVO 11-01 19:48: 日历域 FAB 双入口断言（用户指令——日程+任务不吃笔记流）。 */
const { chromium } = require('/home/z/.npm-global/lib/node_modules/playwright');
const path = require('path');
(async () => {
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 390, height: 844 }, hasTouch: true });
  const errors = [];
  page.on('pageerror', (e) => errors.push(String(e)));
  const BASE = 'file://' + path.resolve(__dirname, '../apps/mobile/dist/index.html');
  const fail = [];
  const ok = (name, cond) => { (cond ? console.log('  ✓ ' + name) : (console.log('  ✗ ' + name), fail.push(name))); };

  await page.goto(BASE); await page.waitForTimeout(1100);
  await page.click('text=日历'); await page.waitForTimeout(500);

  // 1) 日历页: 恰一个 FAB（fab-cal 域内）——无笔记捕获直达
  const fabN = await page.locator('.fab').count();
  ok(`日历页单 FAB（fab-cal 域内,=${fabN}）`, fabN === 1);
  ok('FAB 为日历红 fab-cal', await page.locator('.fab.fab-cal').count() === 1);

  // 20:39 修订: FAB 直连日程（双菜单已拆——任务入口取消）
  await page.click('.fab.fab-cal'); await page.waitForTimeout(450);
  ok('直开: 无菜单无任务入口', await page.locator('.fab-mini').count() === 0 && await page.locator('.fab-mask').count() === 0);

  const evOpen = await page.locator('.sheet-title:has-text("日程")').count() + await page.locator('.sheet:has(input[datetime-local])').count();
  ok('日程→EventEditorSheet 开', evOpen > 0 || /日程/.test(await page.locator('.sheet-title').first().textContent() || ''));
  const dtVal = await page.locator('.sheet input[type="datetime-local"]').first().inputValue().catch(() => '');
  ok(`日程预填 09:00（=${dtVal}）`, /T09:00/.test(dtVal));
  await page.locator('.sheet .icon-btn[aria-label="关闭"]').first().click().catch(() => {}); await page.waitForTimeout(300);

  // 4) 20:39 修订: 任务入口已取消——FAB 只出日程 sheet（不开捕获流）
  await page.click('.fab.fab-cal'); await page.waitForTimeout(450);
  const capTitle = await page.locator('.sheet-title').first().textContent().catch(() => '');
  ok(`FAB 仅日程（标题=${capTitle}, 无任务/捕获流）`, /日程/.test(capTitle || '') && !/快速捕获/.test(capTitle || ''));
  await page.locator('.sheet .icon-btn[aria-label="关闭"]').first().click().catch(() => {}); await page.waitForTimeout(250);

  // 5) 底部散入口已移除
  const oldBtns = await page.locator('button:has-text("为这一天添加任务"), button:has-text("为这一天添加日程")').count();
  ok(`底部 btn-block 散入口已移除（=${oldBtns}）`, oldBtns === 0);

  // 6) 非日历页 FAB 仍笔记捕获（TabBar 精确切页——text=笔记 会误点日历页内文字）
  await page.click('.tab-item:has-text("知识库")'); await page.waitForTimeout(400);
  await page.click('.fab'); await page.waitForTimeout(400);
  const noteCap = await page.locator('.sheet-title').first().textContent().catch(() => '');
  ok(`笔记页 FAB→快速捕获（=${noteCap}）`, /快速捕获/.test(noteCap || ''));

  console.log(`\n=== CALENDAR-FAB-DUAL 验收: ${7 - fail.length}/7 ${fail.length ? 'FAIL: ' + fail.join('|') : 'PASS'}（控制台错 ${errors.length}）===`);
  await page.screenshot({ path: '/tmp/fab-dual.png' });
  await browser.close();
  process.exit(fail.length || errors.length ? 1 : 0);
})();
