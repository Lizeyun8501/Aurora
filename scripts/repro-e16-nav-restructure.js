/**
 * E16 验证（TA 19:13: Agent→右上搜索旁;底部 Agent 位→待办）
 * ① app-bar Agent 钮（搜索旁存在）
 * ② 底部 tab 无 Agent——四项=今日/知识库/待办/日历
 * ③ 点待办→TasksView（任务中心）
 * ④ 点 app-bar Agent→AgentView
 * ⑤ 待办 tab active 判
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

  // ① app-bar Agent 钮
  const barBtns = await page.evaluate(() => {
    const bar = document.querySelector('.app-bar');
    const btns = Array.from(bar.querySelectorAll('button')).map((b) => b.getAttribute('aria-label'));
    return { btns, order: btns.join('|') };
  });
  record('① app-bar 含 Agent 钮（搜索旁）', barBtns.btns.includes('Agent') && barBtns.order.includes('Agent') && barBtns.order.includes('搜索'),
    `钮序=${barBtns.order}`);

  // ② 底部 tab 无 Agent+含待办
  const tabs = await page.evaluate(() => Array.from(document.querySelectorAll('.tab-bar .tab-item')).map((b) => b.textContent?.trim()));
  record('② 底部 nav: 无 Agent 含待办（四项）', !tabs.includes('Agent') && tabs.includes('待办') && tabs.length === 4,
    `tabs=${tabs.join('|')}`);

  // ③ 点待办→TasksView
  await page.click('.tab-bar .tab-item:has-text("待办")');
  await page.waitForTimeout(800);
  const tasksView = await page.evaluate(() => {
    const view = document.querySelector('.view');
    const txt = view ? view.textContent?.slice(0, 60) : '';
    const activeTab = document.querySelector('.tab-bar .tab-item.active')?.textContent?.trim();
    return { txt, activeTab, hasTaskContent: /待办|任务|新增/.test(txt ?? '') };
  });
  record('③ 待办 tab→TasksView+active', tasksView.activeTab === '待办' && tasksView.hasTaskContent,
    `active=${tasksView.activeTab} 内容=${tasksView.txt?.slice(0, 30)}`);

  // ④ app-bar Agent→AgentView
  await page.click('.app-bar button[aria-label="Agent"]');
  await page.waitForTimeout(800);
  const agentView = await page.evaluate(() => {
    const view = document.querySelector('.view');
    const txt = view ? view.textContent?.slice(0, 80) : '';
    return { txt, isAgent: /Agent|模型|配置/.test(txt ?? '') };
  });
  record('④ 右上 Agent→AgentView', agentView.isAgent, `内容=${agentView.txt?.slice(0, 40)}`);

  record('⑤ 零错误', errors.length === 0, errors.slice(0, 2).join(' | ') || 'clean');
  const passed = results.filter((r) => r.p).length;
  console.log(`\n=== E16 导航重构: ${passed}/${results.length} PASS ===`);
  await browser.close();
  process.exit(passed === results.length ? 0 : 1);
})();
