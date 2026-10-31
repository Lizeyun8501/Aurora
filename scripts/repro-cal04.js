/**
 * DK-CALENDAR-04 — Playwright 验证（DoD 六项）
 *
 * ① 周视图 7 列+事件块;② 列头点入切日视图;③ 年视图 12 月+点月切月;
 * ④ 列表分组+筛选+搜索;⑤ sheet 提醒段勾选+闹钟级+reminders 落库往返;
 * ⑥ 今日页「日程」分区;⑦ 零错误。
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

  const today = await (async () => {
    await page.goto('file:///home/z/my-project/repos/Aurora/apps/mobile/dist/index.html');
    await page.waitForTimeout(800);
    return page.evaluate(() => {
      const n = new Date();
      return `${n.getFullYear()}-${String(n.getMonth() + 1).padStart(2, '0')}-${String(n.getDate()).padStart(2, '0')}`;
    });
  })();
  // 明天 key
  const tomorrow = await page.evaluate(() => {
    const n = new Date(); n.setDate(n.getDate() + 1);
    return `${n.getFullYear()}-${String(n.getMonth() + 1).padStart(2, '0')}-${String(n.getDate()).padStart(2, '0')}`;
  });

  // 注入: 今日事件+明日事件（带 note_id）+未来生日
  await page.evaluate(([t, tmr]) => {
    localStorage.setItem('aurora.events', JSON.stringify([
      { id: 'ev-t1', title: '今日评审会', start: `${t}T02:00:00.000Z`, end: null, all_day: false, location: '会议室A', note_id: 'n1', color: 1, repeat: null, reminders: null },
      { id: 'ev-t2', title: '明日写周报', start: `${tmr}T03:00:00.000Z`, end: null, all_day: false, location: null, note_id: null, color: 2, repeat: null, reminders: null },
    ]));
  }, [today, tomorrow]);
  await page.reload();
  await page.waitForTimeout(800);
  {
    const t = await page.$('.tab-bar .tab-item:has-text("日历")');
    if (t) await t.click();
    await page.waitForTimeout(600);
  }

  // ① 周视图: 7 列+今日列高亮+事件块
  await page.click('.cal-mode-seg .nt-seg:nth-child(3)'); // 周
  await page.waitForTimeout(400);
  const week = await page.evaluate(() => ({
    cols: document.querySelectorAll('.week-col').length,
    todayCol: !!document.querySelector('.week-col.today'),
    items: document.querySelectorAll('.week-col .day-axis-item.event').length,
  }));
  record('① 周视图 7 列+今日列+事件块', week.cols === 7 && week.todayCol && week.items >= 1,
    `列=${week.cols} 今日列=${week.todayCol} 块=${week.items}`);

  // ② 周列头点入切日视图
  await page.click('.week-col.today .week-col-head');
  await page.waitForTimeout(400);
  const dayAxis = await page.evaluate(() => ({
    axis: !!document.querySelector('.day-axis'),
    dayOn: document.querySelectorAll('.cal-mode-seg .nt-seg')[3]?.classList.contains('on'),
  }));
  record('② 周列头→日视图', dayAxis.axis && dayAxis.dayOn, `axis=${dayAxis.axis} 日tab=${dayAxis.dayOn}`);

  // ③ 年视图: 12 月+点月切月
  await page.click('.cal-title'); // 年月标题进年
  await page.waitForTimeout(400);
  const year = await page.evaluate(() => ({
    months: document.querySelectorAll('.year-month').length,
  }));
  const months12 = year.months === 12;
  await page.evaluate(() => document.querySelectorAll('.year-month')[new Date().getMonth()].click());
  await page.waitForTimeout(400);
  const backMonth = await page.evaluate(() => !!document.querySelector('.cal-grid'));
  record('③ 年视图 12 月+点月切月', months12 && backMonth, `月格=${year.months} 回月=${backMonth}`);

  // ④ 列表: 分组+筛选+搜索
  await page.click('.cal-mode-seg .nt-seg:nth-child(5)'); // 日程
  await page.waitForTimeout(400);
  const list = await page.evaluate(() => ({
    groups: [...document.querySelectorAll('.ev-list-head')].map((e) => e.textContent),
    items: document.querySelectorAll('.ev-list-item').length,
    badges: document.querySelectorAll('.ev-list-badge').length,
  }));
  record('④a 列表分组（今天/明天）+关联徽标', list.groups.includes('今天') && list.groups.includes('明天') && list.badges >= 1,
    `组=${list.groups.join(',')} 条=${list.items} 徽=${list.badges}`);
  // 筛选: 待办关联
  await page.click('.ev-list-filters .nt-seg:nth-child(2)');
  await page.waitForTimeout(300);
  const filtered = await page.evaluate(() => document.querySelectorAll('.ev-list-item').length);
  record('④b 筛选「待办关联」', filtered === 1, `筛后=${filtered}（应 1——带 note_id）`);
  await page.click('.ev-list-filters .nt-seg:nth-child(1)'); // 回全
  // 搜索
  await page.fill('input[aria-label="日程搜索"]', '周报');
  await page.waitForTimeout(300);
  const searched = await page.evaluate(() => document.querySelectorAll('.ev-list-item').length);
  record('④c 搜索「周报」', searched === 1, `命中=${searched}`);
  await page.fill('input[aria-label="日程搜索"]', '');

  // ⑤ sheet 提醒段: 勾选 30 分钟+闹钟级+落库往返
  await page.click('.cal-mode-seg .nt-seg:nth-child(2)'); // 月
  await page.waitForTimeout(300);
  await page.click('.cal-day.today');
  await page.waitForTimeout(200);
  await page.click('.btn.btn-primary.btn-block:has-text("为这一天添加日程")');
  await page.waitForTimeout(400);
  await page.fill('input[aria-label="日程标题"]', '提醒测试日程');
  await page.click('.ev-rem-btn:has-text("30 分钟")');
  await page.waitForTimeout(200);
  const alarmBtn = await page.$('.ev-rem-alarm');
  if (alarmBtn) await alarmBtn.click();
  await page.waitForTimeout(200);
  await page.click('button:has-text("保存日程")');
  await page.waitForTimeout(500);
  const stored = await page.evaluate(() => {
    const evs = JSON.parse(localStorage.getItem('aurora.events') ?? '[]');
    const ev = evs.find((e) => e.title === '提醒测试日程');
    return ev ? { n: ev.reminders?.length ?? 0, alarm: ev.reminders?.[0]?.alarm_level ?? 0, off: ev.reminders?.[0]?.offset_min ?? -1 } : null;
  });
  record('⑤ 提醒段勾选+闹钟级+落库往返', !!stored && stored.n === 1 && stored.alarm === 1 && stored.off === 30,
    stored ? `档=${stored.n} 铃=${stored.alarm} 偏移=${stored.off}分` : '未落库');

  // ⑥ 今日页「日程」分区
  {
    const t = await page.$('.tab-bar .tab-item:has-text("今日")');
    if (t) await t.click();
    await page.waitForTimeout(500);
  }
  const todaySec = await page.evaluate(() => {
    const head = [...document.querySelectorAll('.task-section-head')].find((h) => h.textContent?.includes('今日日程'));
    const items = head ? head.parentElement.querySelectorAll('.ev-list-item').length : 0;
    return { head: !!head, items };
  });
  record('⑥ 今日页日程分区', todaySec.head && todaySec.items >= 2,
    `分区=${todaySec.head} 条=${todaySec.items}（今日评审会+提醒测试日程）`);

  record('⑦ 零错误', errors.length === 0, errors.length ? errors.slice(0, 2).join(' | ') : '双监听 0');

  const failed = results.filter((r) => !r.pass).length;
  console.log(`\n=== CALENDAR-04 验收: ${results.length - failed}/${results.length} PASS ===`);
  await browser.close();
  process.exit(failed > 0 ? 1 : 0);
})();
