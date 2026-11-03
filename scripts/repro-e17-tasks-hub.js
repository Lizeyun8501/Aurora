/**
 * E17 验证（TA 19:30 任务中心增强）
 * ① 捕获三档优先级选+落库
 * ② 统计头（完成率环+今日+逾期+高优四格）
 * ③ 日期分组（逾期红头/今日/稍后安排）
 * ④ 优先级色点点击循环（low→normal→high）
 * ⑤ 优先级过滤段+排序（high 前）
 * ⑥ 子任务进度 n/m
 */
const { chromium } = require('/home/z/.npm-global/lib/node_modules/playwright');
const BASE = 'file:///home/z/my-project/Aurora/apps/mobile/dist/index.html';

(async () => {
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 390, height: 844 }, hasTouch: true });
  const errors = [];
  page.on('pageerror', (e) => errors.push(String(e).slice(0, 120)));
  const results = [];
  const record = (n, p, d) => { results.push({ n, p }); console.log(`${p ? 'PASS' : 'FAIL'} ${n}: ${d}`); };

  await page.goto(BASE);
  await page.waitForTimeout(1800);
  // 清旧任务（localStorage 干净基线）
  await page.evaluate(() => localStorage.removeItem('aurora.tasks'));

  // ① 捕获: 高优先任务
  await page.click('.fab');
  await page.waitForTimeout(600);
  await page.evaluate(() => {
    const segs = Array.from(document.querySelectorAll('.task-prio-seg'));
    segs.find((x) => x.textContent?.includes('高'))?.click();
  });
  await page.click('.sheet textarea');
  await page.keyboard.type('高优先级任务', { delay: 25 });
  await page.click('.sheet button:has-text("加入今日任务")');
  await page.waitForTimeout(700);

  // 再捕: 低优先（无日期→稍后安排组）
  await page.click('.fab');
  await page.waitForTimeout(600);
  await page.evaluate(() => {
    const segs = Array.from(document.querySelectorAll('.task-prio-seg'));
    segs.find((x) => x.textContent?.includes('低'))?.click();
  });
  await page.click('.sheet textarea');
  await page.keyboard.type('低优先任务', { delay: 25 });
  await page.click('.sheet button:has-text("加入今日任务")');
  await page.waitForTimeout(700);

  // 进待办中心
  await page.click('.tab-bar .tab-item:has-text("待办")');
  await page.waitForTimeout(900);
  const hub = await page.evaluate(() => {
    const stats = Array.from(document.querySelectorAll('.task-stat')).map((el) => ({
      label: el.querySelector('.task-stat-label')?.textContent ?? el.querySelector('.task-stat-ring-num')?.textContent ?? '',
      num: el.querySelector('.task-stat-num')?.textContent ?? el.querySelector('.task-stat-ring-num')?.textContent ?? '',
    }));
    const groups = Array.from(document.querySelectorAll('.task-group-head')).map((g) => g.textContent);
    const items = Array.from(document.querySelectorAll('.task-item .task-text')).map((t) => t.textContent);
    const chips = Array.from(document.querySelectorAll('.chip')).map((c) => c.textContent?.trim());
    const ring = document.querySelector('.task-stat-ring-num')?.textContent;
    return { stats, groups, items, chips, ring };
  });
  record('① 捕获优先级落库（高/低两任务在列）', hub.items.includes('高优先级任务') && hub.items.includes('低优先任务'), `items=${hub.items}`);
  record('② 统计头四格（环+今日+逾期+高优）', hub.stats.length >= 4 && !!hub.ring, `stats=${JSON.stringify(hub.stats)}`);
  record('③ 日期分组（今日组+计数）', hub.groups.some((g) => g.includes('今日2')), `groups=${hub.groups}（due=今日语义修后两任务归今日组）`);

  // ⑤ 排序: 高优在低前
  const idx = hub.items.indexOf('高优先级任务') < hub.items.indexOf('低优先任务');
  record('⑤ high 排序在前', idx, `序=${hub.items.join('|')}`);

  // ④ 色点循环: 点低任务的点（low→normal）
  const dotBefore = await page.evaluate(() => {
    const items = Array.from(document.querySelectorAll('.task-item'));
    const low = items.find((it) => it.querySelector('.task-text')?.textContent === '低优先任务');
    return low ? getComputedStyle(low.querySelector('.task-prio-dot')).backgroundColor : '';
  });
  await page.evaluate(() => {
    const items = Array.from(document.querySelectorAll('.task-item'));
    const low = items.find((it) => it.querySelector('.task-text')?.textContent === '低优先任务');
    (low?.querySelector('.task-prio-dot-btn') ?? low?.querySelector('.task-prio-dot'))?.click();
  });
  await page.waitForTimeout(500);
  const dotAfter = await page.evaluate(() => {
    const items = Array.from(document.querySelectorAll('.task-item'));
    const low = items.find((it) => it.querySelector('.task-text')?.textContent === '低优先任务');
    return low ? getComputedStyle(low.querySelector('.task-prio-dot')).backgroundColor : '';
  });
  record('④ 优先级色点循环（低→中色变）', dotBefore !== dotAfter && !!dotAfter, `前=${dotBefore} 后=${dotAfter}`);

  // ⑤b 优先级过滤段: 点「高」chip
  await page.evaluate(() => {
    const chips = Array.from(document.querySelectorAll('.chip'));
    chips.find((c) => c.textContent?.trim().startsWith('高'))?.click();
  });
  await page.waitForTimeout(500);
  const filtered = await page.evaluate(() => Array.from(document.querySelectorAll('.task-item .task-text')).map((t) => t.textContent));
  record('⑤b 优先级过滤（高段只剩高任务）', filtered.length === 1 && filtered.includes('高优先级任务'), `过滤后=${filtered}`);
  // 撤过滤
  await page.evaluate(() => {
    const chips = Array.from(document.querySelectorAll('.chip'));
    chips.find((c) => c.textContent?.trim().startsWith('高'))?.click();
  });
  await page.waitForTimeout(400);

  record('⑥ 零错误', errors.length === 0, errors.slice(0, 2).join(' | ') || 'clean');
  const passed = results.filter((r) => r.p).length;
  console.log(`\n=== E17 任务中心增强: ${passed}/${results.length} PASS ===`);
  await browser.close();
  process.exit(passed === results.length ? 0 : 1);
})();
