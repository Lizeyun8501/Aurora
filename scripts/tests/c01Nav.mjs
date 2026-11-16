#!/usr/bin/env node
/**
 * C-01/ADR-007 移动导航断言套件——PW file:// 驱动（B-05 e2eTopPaths 同通道）。
 * 断言面: 五 Tab+a11y（tablist/aria-selected/双载体）/Inbox 主入口+捕获去处/
 * 设置二级（笔记页齿轮）/Agent→抽屉/⌘K/右下 FAB 无中央凸起+键盘避让。
 */
import { createRequire } from 'module';
import { fileURLToPath } from 'url';
import path from 'path';
let chromium;
try {
  ({ chromium } = createRequire(import.meta.url)('playwright'));
} catch {
  ({ chromium } = createRequire('/home/z/.npm-global/lib/node_modules/playwright/')('playwright'));
}
const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const BASE = process.env.AURORA_E2E_BASE
  ?? 'file://' + path.join(repoRoot, 'apps', 'mobile', 'dist', 'index.html');

let pass = 0, fail = 0;
const ok = (name, cond, extra = '') => {
  if (cond) { pass++; console.log(`PASS ${name}`); }
  else { fail++; console.log(`FAIL ${name}${extra ? ' — ' + extra : ''}`); }
};

const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 390, height: 844 } });
page.on('pageerror', (e) => console.log('PAGE-ERR:', e.message));
await page.goto(BASE);
await page.waitForSelector('.tab-bar', { timeout: 20000 });

// ① 五 Tab——今日/笔记/收集箱/任务/日历（ADR-007 单一事实源）
const tabs = await page.$$('[role="tablist"][aria-label="主导航"] [role="tab"]');
ok('①tablist 存在+role=tab', tabs.length === 5, `tabs=${tabs.length}`);
const labels = await page.$$eval('[role="tablist"] [role="tab"] span', (ns) => ns.map((n) => n.textContent));
ok('①五 Tab 标签序', JSON.stringify(labels) === JSON.stringify(['今日', '笔记', '收集箱', '任务', '日历']), JSON.stringify(labels));
const icons = await page.$$eval('[role="tablist"] [role="tab"] svg', (ns) => ns.length);
ok('①图标文字双载体（svg×5）', icons === 5, `svg=${icons}`);

// ② a11y: aria-selected 联动（今日默认选中;点收集箱翻转）
const sel1 = await page.getAttribute('[role="tablist"] [role="tab"]:nth-child(1)', 'aria-selected');
ok('②今日默认 aria-selected=true', sel1 === 'true', sel1 ?? 'null');
await page.click('[role="tablist"] [role="tab"]:nth-child(3)');
await page.waitForTimeout(300);
const selNow = await page.$$eval('[role="tablist"] [role="tab"]', (ns) => ns.map((n) => n.getAttribute('aria-selected')));
ok('②点击收集箱后 aria-selected 翻转', selNow[2] === 'true' && selNow[0] === 'false', JSON.stringify(selNow));
const title = await page.textContent('.app-bar-title');
ok('②收集箱视图标题', title === '收集箱', title ?? '');

// ③ Inbox 空态→捕获→存入收集箱→收集箱出现（捕获内容有明确去处）
const empty = await page.textContent('.inbox-view');
ok('③收集箱空态可见', empty.includes('收集箱是空的'), (empty ?? '').slice(0, 40));
await page.click('button.fab[aria-label="捕获"]');
await page.waitForSelector('.sheet', { timeout: 8000 });
await page.fill('.sheet textarea, .sheet input[type="text"]', 'C01测试-收集条目');
// 存入收集箱按钮（GTD 收集段主按钮）
const stashBtn = await page.$$eval('.sheet .btn', (ns) => ns.map((n) => n.textContent).find((t) => t.includes('存入收集箱')));
ok('③捕获面板「存入收集箱」按钮在', !!stashBtn);
await page.click('.sheet .btn:has-text("存入收集箱")');
await page.waitForTimeout(500);
const itemCount = await page.$$eval('.inbox-list .inbox-item', (ns) => ns.length);
ok('③存入后收集箱有条目', itemCount === 1, `items=${itemCount}`);

// ④ 处理动作: 转任务（今日任务可见）——GTD 处理段
await page.click('.inbox-item .btn:has-text("转任务")');
await page.waitForTimeout(500);
const gone = await page.$$eval('.inbox-list .inbox-item', (ns) => ns.length);
ok('④转任务后收集箱清空', gone === 0, `items=${gone}`);
await page.click('[role="tablist"] [role="tab"]:nth-child(1)');
await page.waitForTimeout(400);
const todayText = await page.textContent('main');
ok('④今日视图可见转出任务', todayText.includes('C01测试-收集条目'), '');

// ⑤ 设置二级: TabBar 无设置;笔记页有齿轮;非笔记页无齿轮
const tabTexts = await page.$$eval('[role="tablist"] [role="tab"]', (ns) => ns.map((n) => n.textContent));
ok('⑤TabBar 无设置项', !tabTexts.some((t) => t.includes('设置')), JSON.stringify(tabTexts));
await page.click('[role="tablist"] [role="tab"]:nth-child(2)');
await page.waitForTimeout(400);
const gearOnNotes = await page.$$('[aria-label="设置"]');
ok('⑤笔记页设置齿轮在', gearOnNotes.length === 1, `gear=${gearOnNotes.length}`);
await page.click('[role="tablist"] [role="tab"]:nth-child(1)');
await page.waitForTimeout(300);
const gearOnToday = await page.$$('[aria-label="设置"]');
ok('⑤今日页无设置齿轮（仅笔记页）', gearOnToday.length === 0, `gear=${gearOnToday.length}`);

// ⑥ Agent→抽屉「更多」;AppBar 无 Agent
const appBarAgent = await page.$$('.app-bar [aria-label="Agent"]');
ok('⑥AppBar 无 Agent 按钮', appBarAgent.length === 0);
await page.click('.app-bar [aria-label="菜单"]');
await page.waitForSelector('.drawer, aside', { timeout: 8000 });
const drawerText = await page.evaluate(() => document.body.textContent);
ok('⑥抽屉「更多」组含 Agent', drawerText.includes('Agent'), '');
const drawerAgentBtn = await page.$$('.d-item, aside button');
ok('⑥抽屉可交互项存在', drawerAgentBtn.length > 0, `btns=${drawerAgentBtn.length}`);
await page.keyboard.press('Escape');
await page.click('.sheet-mask, .drawer-mask', { force: true }).catch(() => {});
await page.waitForTimeout(300);

// ⑦ ⌘K → 搜索视图（硬件键盘等价快捷——ADR-007）
await page.keyboard.press('Control+k');
await page.waitForTimeout(400);
const searchTitle = await page.textContent('.app-bar-title');
ok('⑦Ctrl+K 进搜索', searchTitle === '搜索', searchTitle ?? '');

// ⑧ FAB 右下无中央凸起 + 键盘避让
const fabBox = await page.$eval('button.fab[aria-label="捕获"]', (n) => n.getBoundingClientRect());
ok('⑧FAB 在右半区（非中央凸起）', fabBox.right > 390 / 2, `right=${Math.round(fabBox.right)}`);
/* 安全区: env() 在无刘海环境计算值为 0（计算断言不可用）——改断样式表
   规则存在（作者级声明 .tab-bar { padding-bottom: env(safe-area-inset-bottom) }）*/
const safeAreaRule = await page.evaluate(() => {
  for (const sheet of document.styleSheets) {
    let rules; try { rules = sheet.cssRules; } catch { continue; }
    for (const r of rules) {
      if (r.selectorText === '.tab-bar' && r.style?.paddingBottom?.includes('safe-area-inset-bottom')) return true;
    }
  }
  return false;
});
ok('⑧TabBar 安全区规则（env(safe-area-inset-bottom)）', safeAreaRule === true);
// 键盘避让: 模拟 visualViewport 收缩（真机软键盘）→ FAB 隐藏
const hidden = await page.evaluate(() => {
  const vv = window.visualViewport;
  if (!vv) return 'no-vv';
  Object.defineProperty(vv, 'height', { value: 400, configurable: true });
  vv.dispatchEvent(new Event('resize'));
  return new Promise((r) => setTimeout(() => {
    const fab = document.querySelector('button.fab[aria-label="捕获"]');
    r(fab ? getComputedStyle(fab).display : 'missing');
  }, 250));
});
ok('⑧键盘避让: FAB display=none', hidden === 'none', hidden);
// 恢复视口
await page.evaluate(() => {
  const vv = window.visualViewport;
  Object.defineProperty(vv, 'height', { value: 844, configurable: true });
  vv.dispatchEvent(new Event('resize'));
});

// ⑨ 回归: localStorage 清（遗留态污染防——B-05 纪律）
await page.evaluate(() => { localStorage.removeItem('aurora.inbox'); });

console.log(`\n=== C-01 导航断言: ${pass}/${pass + fail} ${fail === 0 ? 'PASS' : 'FAIL'} ===`);
await browser.close();
process.exit(fail === 0 ? 0 : 1);
