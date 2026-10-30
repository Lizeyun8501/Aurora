/**
 * DK-CALENDAR-02 — Playwright 全链路验证（DoD 2/3/4）
 *
 * 环境: apps/mobile/dist/index.html + adapter mock localStorage 路径
 * （platform 无 AndroidBridge → mock 'aurora.events' CRUD——行为级断言
 * 不依赖真机;真桥 arm64 交叉编译/gradle 由 CI 终证）。
 *
 * 链路: 新建事件 → 月历角标 → 日视图事件块 → 编辑 → 删除。
 */
const BASE = require('fs').existsSync('/home/z/my-project/repos/Aurora/apps/mobile/dist/index.html')
  ? 'file:///home/z/my-project/repos/Aurora/apps/mobile/dist/index.html'
  : 'file:///home/z/my-project/Aurora/apps/mobile/dist/index.html';
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

  await page.goto(BASE);
  await page.waitForTimeout(800);

  // ① 进日历 Tab
  {
    const tab = await page.$('.tab-bar .tab-item:has-text("日历")');
    if (tab) { await tab.click(); await page.waitForTimeout(500); }
    const grid = await page.$('.cal-grid');
    record('① 月历渲染', !!tab && !!grid, `tab=${!!tab} grid=${!!grid}`);
  }

  const today = new Date();
  const pad = (n) => String(n).padStart(2, '0');
  const todayKey = `${today.getFullYear()}-${pad(today.getMonth() + 1)}-${pad(today.getDate())}`;

  // ② 「为这一天添加日程」→ sheet 打开 → 填标题保存
  {
    const addBtn = await page.$('button:has-text("为这一天添加日程")');
    if (addBtn) { await addBtn.click(); await page.waitForTimeout(400); }
    const sheet = await page.$('.sheet');
    const titleInput = await page.$('.sheet input[aria-label="日程标题"]');
    if (titleInput) { await titleInput.fill('端到端测试日程'); }
    const saveBtn = await page.$('button:has-text("保存日程")');
    if (saveBtn) { await saveBtn.click(); await page.waitForTimeout(500); }
    const stored = await page.evaluate(() => localStorage.getItem('aurora.events') ?? '[]');
    const evs = JSON.parse(stored);
    record('② 新建事件→mock 落库', !!sheet && evs.length === 1 && evs[0].title === '端到端测试日程',
      `sheet=${!!sheet} 落库=${evs.length}条 title=${evs[0]?.title ?? '-'}`);
  }

  // ③ 月历角标（今日格 .cal-ev-dots 出现+色点）
  {
    const dots = await page.$('.cal-day.today .cal-ev-dots');
    record('③ 月历事件角标', !!dots, `today 格色点=${!!dots}`);
  }

  // ④ 切日视图→09:00 槽事件块（start 预填 09:00）
  {
    const dayTab = await page.$('.cal-mode-seg button:has-text("日")');
    if (dayTab) { await dayTab.click(); await page.waitForTimeout(500); }
    const evItem = await page.$('.day-axis-item.event');
    const text = await page.evaluate(() => document.querySelector('.day-axis-item.event')?.textContent ?? '');
    record('④ 日视图事件块', !!evItem && text.includes('端到端测试日程'), `块=${!!evItem} text=${text}`);
  }

  // ⑤ 事件块点击→编辑 sheet→改标题保存
  {
    const evItem = await page.$('.day-axis-item.event');
    if (evItem) { await evItem.click(); await page.waitForTimeout(400); }
    const titleInput = await page.$('.sheet input[aria-label="日程标题"]');
    const sheetTitle = await page.evaluate(() => document.querySelector('.sheet-title')?.textContent ?? '');
    if (titleInput) { await titleInput.fill('改后日程'); }
    const saveBtn = await page.$('button:has-text("保存日程")');
    if (saveBtn) { await saveBtn.click(); await page.waitForTimeout(500); }
    const stored = await page.evaluate(() => localStorage.getItem('aurora.events') ?? '[]');
    const evs = JSON.parse(stored);
    record('⑤ 编辑往返', sheetTitle === '编辑日程' && evs[0]?.title === '改后日程',
      `sheetTitle=${sheetTitle} 改后=${evs[0]?.title}`);
  }

  // ⑥ 删除（事件块→sheet→确认→mock 清空）
  {
    const dayTab = await page.$('.cal-mode-seg button:has-text("日")');
    if (dayTab) { await dayTab.click(); await page.waitForTimeout(400); }
    const evItem = await page.$('.day-axis-item.event');
    if (evItem) { await evItem.click(); await page.waitForTimeout(400); }
    page.on('dialog', (d) => d.accept());
    const delBtn = await page.$('button:has-text("删除日程")');
    if (delBtn) { await delBtn.click(); await page.waitForTimeout(500); }
    const stored = await page.evaluate(() => localStorage.getItem('aurora.events') ?? '[]');
    const evs = JSON.parse(stored);
    const noDots = await page.evaluate(() => {
      const t = document.querySelector('.cal-day.today .cal-ev-dots');
      return t === null;
    });
    record('⑥ 删除→空态', evs.length === 0 && noDots, `落库=${evs.length}条 角标消失=${noDots}`);
  }

  // ⑦ errors === 0
  record('⑦ 零错误', errors.length === 0, errors.length ? errors.slice(0, 2).join(' | ') : '双监听 0');

  const failed = results.filter((r) => !r.pass).length;
  console.log(`\n=== CALENDAR-02 验收: ${results.length - failed}/${results.length} PASS ===`);
  await browser.close();
  process.exit(failed > 0 ? 1 : 0);
})();
