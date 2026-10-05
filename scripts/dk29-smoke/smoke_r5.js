#!/usr/bin/env node
/* DK-35 r5：桌面冒烟（DK-29 r4 延续——A5 SmartFolder SKIP 缺口补齐）
 * 变更：mock 态样本树（🔮 全部待办）+ mock invoke 数据面 → A5 从 SKIP 升级为 DOM 级 PASS
 * 口径：与 r4 相同（Xvfb :78 + playwright headful Chromium + http server——vite dist 需 http 加载）
 */
const { chromium } = require('/home/z/.npm-global/lib/node_modules/playwright');
const { execSync, spawn } = require('child_process');

const results = [];
const record = (name, pass, detail) => {
  results.push({ name, pass });
  console.log(`${pass ? 'PASS' : 'FAIL'} ${name}: ${detail}`);
};

(async () => {
  const browser = await chromium.launch({ headless: false, args: ['--no-sandbox', '--disable-dev-shm-usage'] });
  const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
  const pageErrors = [];
  page.on('pageerror', (e) => pageErrors.push(String(e)));

  await page.goto('http://127.0.0.1:8977', { waitUntil: 'networkidle' });
  await page.waitForTimeout(1200);

  // A2 导航到今日视图 → WeeklyReview 面板（r4 同款导航——A1 根挂载合并检查）
  const rootMounted = await page.evaluate(() => !!document.querySelector('#root') && document.querySelector('#root').children.length > 0);
  record('A1 DesktopShell根挂载', rootMounted, `root_children=${await page.evaluate(() => document.querySelector('#root').children.length)}`);
  await page.locator('nav[aria-label="主导航"] button', { hasText: '今日视图' }).click();
  await page.waitForTimeout(1200);
  const weeklyVisible = await page.locator('h2', { hasText: '周回顾' }).count();
  record('A2 WeeklyReview面板可达(DK-25)', weeklyVisible > 0, `h2_周回顾=${weeklyVisible}`);

  // A5 升级：mock 样本树侧栏节点点击 → SmartFolderView DOM 断言
  let a5pass = false, a5detail = '';
  try {
    const sfItem = page.locator('text=全部待办').first();
    await sfItem.click({ timeout: 5000 });
    await page.waitForTimeout(800);
    const sfHeader = await page.locator('h1', { hasText: '🔮' }).count();
    const itemCount = await page.locator('text=共 2 篇').count();
    const ruleInput = await page.locator('input[aria-label*="标题包含"]').count();
    a5pass = sfHeader > 0 && itemCount > 0 && ruleInput > 0;
    a5detail = `🔮标题=${sfHeader} 共2篇=${itemCount} 规则输入框=${ruleInput}`;
  } catch (e) {
    a5detail = `点击后断言异常: ${String(e).slice(0, 80)}`;
  }
  record('A5 SmartFolderView mock数据面DOM断言(DK-27/DK-35)', a5pass, a5detail);

  // A6 规则回显（mock get_rule: title_contains='待办'）
  let a6pass = false, a6detail = '';
  try {
    const dv = await page.locator('input[aria-label*="标题包含"]').inputValue({ timeout: 5000 });
    a6pass = dv === '待办';
    a6detail = `规则回显值=${JSON.stringify(dv)}`;
  } catch (e) {
    a6detail = `回显断言异常: ${String(e).slice(0, 80)}`;
  }
  record('A6 SmartFolder规则回显(mock get_rule)', a6pass, a6detail);

  // A7 列表项渲染（mock list_items 两条）
  let a7pass = false, a7detail = '';
  try {
    const n1 = await page.locator('text=周一站会纪要').count();
    const n2 = await page.locator('text=DK-33 依赖盘点').count();
    a7pass = n1 > 0 && n2 > 0;
    a7detail = `样本项1=${n1} 样本项2=${n2}`;
  } catch (e) {
    a7detail = `列表断言异常: ${String(e).slice(0, 80)}`;
  }
  record('A7 SmartFolder求值列表渲染(mock 2条)', a7pass, a7detail);

  // A8 全程 pageerror=0
  record('A8 页面零错误', pageErrors.length === 0, `pageerrors=${pageErrors.length}`);

  await browser.close();
  const pass = results.filter((r) => r.pass).length;
  console.log(`SUMMARY ${pass}/${results.length}`);
  process.exit(pass === results.length ? 0 : 1);
})();
