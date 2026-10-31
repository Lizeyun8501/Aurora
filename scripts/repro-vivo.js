/* repro-vivo.js — CALENDAR-VIVO 视觉对齐断言（10-31 用户五图裁决快修）
 * 六断言: 月红大标题/胶囊分段白底/选中红圈/日视图红时间线/列表白卡圆角/年今日月红
 */
const { chromium } = require('/home/z/.npm-global/lib/node_modules/playwright');

(async () => {
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 390, height: 844 }, hasTouch: true });
  const errors = [];
  const results = [];
  const record = (name, pass, detail) => {
    results.push({ name, pass, detail });
    console.log(`${pass ? 'PASS' : 'FAIL'} ${name}: ${detail}`);
  };
  page.on('pageerror', (e) => errors.push(String(e)));
  page.on('console', (m) => { if (m.type() === 'error') errors.push(m.text()); });

  await page.goto('file:///home/z/my-project/repos/Aurora/apps/mobile/dist/index.html');
  await page.waitForTimeout(1200);
  await page.click('text=日历');
  await page.waitForTimeout(600);

  // ① 顶栏: 月数字红大标题
  const mEl = await page.$('.cal-title-m');
  const mColor = mEl ? await mEl.evaluate((el) => getComputedStyle(el).color) : 'none';
  const mSize = mEl ? await mEl.evaluate((el) => getComputedStyle(el).fontSize) : '0';
  record('① 月数字红大标题', mColor === 'rgb(255, 77, 79)' && parseInt(mSize) >= 24, `${mColor} ${mSize}`);

  // ② 胶囊分段: seg 容器圆角 999+五态
  const seg = await page.$('.cal-mode-seg');
  const segR = seg ? await seg.evaluate((el) => getComputedStyle(el).borderRadius) : 'none';
  const segLen = await page.$$eval('.cal-mode-seg .nt-seg', (els) => els.length).catch(() => 0);
  record('② 胶囊分段+五态', segR.includes('999') && segLen === 5, `radius=${segR} 段数=${segLen}`);

  // ③ 选中红圈（今日默认选中——vivo 红底白字圈）
  const selNum = await page.$('.cal-day-btn.sel .cal-num');
  const sBg = selNum ? await selNum.evaluate((el) => getComputedStyle(el).backgroundColor) : 'none';
  const sRad = selNum ? await selNum.evaluate((el) => getComputedStyle(el).borderRadius) : 'none';
  record('③ 选中红圈（今日默认）', sBg === 'rgb(255, 77, 79)' && sRad.includes('50%'), `bg=${sBg} rad=${sRad}`);

  // ④ 日视图红当前时间线（今日 08-22 内应渲染）
  await page.click('.cal-mode-seg .nt-seg:nth-child(4)'); // 日
  await page.waitForTimeout(400);
  const nowLine = await page.$('.day-now-line');
  const nlBorder = nowLine ? await nowLine.evaluate((el) => getComputedStyle(el).borderTopColor) : 'none';
  record('④ 日视图红时间线', nowLine !== null && nlBorder === 'rgb(255, 77, 79)', nowLine ? `border=${nlBorder}` : '未渲染(今日08-22外?)');

  // ⑤ 日程列表白卡大圆角（种今日事件保 item 渲染）
  await page.evaluate(() => {
    const d = new Date();
    const key = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
    localStorage.setItem('aurora.events', JSON.stringify([{
      id: 'ev-vivo', title: '对齐验证事件', start: key + 'T10:00:00Z', end: null,
      all_day: false, location: null, note_id: null, color: 2, repeat: null, reminders: null,
    }]));
  });
  await page.reload();
  await page.waitForTimeout(1200);
  await page.click('text=日历');
  await page.waitForTimeout(500);
  await page.click('.cal-mode-seg .nt-seg:nth-child(5)'); // 日程
  await page.waitForTimeout(500);
  const item = await page.$('.ev-list-item');
  const itemR = item ? await item.evaluate((el) => getComputedStyle(el).borderRadius) : 'none';
  const itemBg = item ? await item.evaluate((el) => getComputedStyle(el).backgroundColor) : 'none';
  record('⑤ 列表白卡14px圆角', item !== null && itemR.startsWith('14'), `radius=${itemR} bg=${itemBg}`);

  // ⑥ 年视图今日月红
  await page.click('.cal-mode-seg .nt-seg:nth-child(1)'); // 年
  await page.waitForTimeout(400);
  const curYM = await page.$('.year-month.cur .ym-name');
  const curColor = curYM ? await curYM.evaluate((el) => getComputedStyle(el).color) : 'none';
  record('⑥ 年视图今日月红', curColor === 'rgb(255, 77, 79)', curColor);

  const passed = results.filter((r) => r.pass).length;
  console.log(`\n=== CALENDAR-VIVO 验收: ${passed}/${results.length} PASS（控制台错 ${errors.length}） ===`);
  await browser.close();
  process.exit(passed === results.length && errors.length === 0 ? 0 : 1);
})();
