/**
 * R19 移动端块插入面板 — Playwright 验收（任务书 DoD 核心）
 *
 * 环境: dist-r19/r19-lab.html（React+RichEditor+mock 桥, --kbd-inset 注入 0）。
 * 断言面: dock 五键 / + 面板开合 / 逐项插入 DOM / 撤销回退 / 块上下移 swap /
 *         pageerror + console.error 双监听 0 错误。
 */
const { chromium } = require('/home/z/.npm-global/lib/node_modules/playwright');

(async () => {
  const browser = await chromium.launch();
  const page = await browser.newPage({
    viewport: { width: 390, height: 844 }, // 移动视口
    hasTouch: true,
  });

  const errors = [];
  page.on('pageerror', (e) => errors.push(`pageerror: ${e.message}`));
  page.on('console', (m) => { if (m.type() === 'error') errors.push(`console.error: ${m.text()}`); });

  const results = [];
  const record = (name, pass, detail) => {
    results.push({ name, pass, detail });
    console.log(`${pass ? 'PASS' : 'FAIL'} ${name}: ${detail}`);
  };

  await page.goto('file:///home/z/my-project/Aurora/apps/mobile/dist-r19/r19-lab.html');
  await page.waitForFunction(() => window.__editorStatus === 'rich', null, { timeout: 15000 });
  record('lab rich 态加载', true, 'editorStatus=rich');

  // ① dock 挂载 + 五键
  {
    const hasDock = await page.evaluate(() => window.__probe.dockVisible());
    const labels = await page.evaluate(() =>
      Array.from(document.querySelectorAll('.m-dock button')).map((b) => b.getAttribute('aria-label')));
    record('①a dock 挂载', hasDock, 'm-dock 存在');
    record('①b dock 五键', ['插入块', '撤销', '重做', '块上移', '块下移'].every((l) => labels.includes(l)),
      JSON.stringify(labels));
  }

  // ② + 开面板 + 13 项
  {
    await page.click('.m-dock button[aria-label="插入块"]');
    await page.waitForTimeout(220); // transition 180ms
    const open = await page.evaluate(() => window.__probe.panelOpen());
    const items = await page.evaluate(() => window.__probe.sheetItems());
    record('②a 面板开', open, 'm-sheet.open');
    record('②b 13 菜单项', items.length === 13, `${items.length} 项: ${items.join(',')}`);
    // 关面板（mask 点击）——③ 循环每轮再开（点 dock + 需面板关闭态）
    await page.click('.m-sheet-mask', { position: { x: 195, y: 60 } });
    await page.waitForTimeout(220);
  }

  // ③ 逐项插入（DOM 断言）——每项在面板点一次，断言编辑区出现对应元素
  // 断言面：全文档选择器（插入可能落容器内——blockquote>code_block 等合法嵌套）
  {
    const cases = [
      { label: '标题一', sel: 'h1', detail: 'h1' },
      { label: '标题二', sel: 'h2', detail: 'h2' },
      { label: '无序列表', sel: 'ul', detail: 'ul' },
      { label: '有序列表', sel: 'ol', detail: 'ol' },
      { label: '任务列表', sel: '.task-block', detail: 'task-block' },
      { label: '引述块', sel: 'blockquote', detail: 'blockquote' },
      { label: '代码块', sel: 'pre', detail: 'pre' },
      { label: '表格', sel: 'table', detail: 'table' },
      { label: '分割线', sel: 'hr', detail: 'hr' },
    ];
    for (const c of cases) {
      // 打开面板（上次插入后已关）
      await page.click('.m-dock button[aria-label="插入块"]');
      await page.waitForTimeout(200);
      await page.click(`.m-grid button[aria-label="${c.label}"]`);
      await page.waitForTimeout(80);
      const has = await page.evaluate((sel) => window.__probe.has(sel), c.sel);
      record(`③ 插入${c.label}`, has, `${c.sel} 出现`);
    }
  }

  // ④ 撤销：末次插入（分割线）可回退
  // 语义注记：undo 源自 LoroUndoPlugin（CRDT 侧）——快间隔连续插入（<合并窗）
  // 可能被合并为一步撤销（撤销一次回退多块）。断言放宽：撤销可回退+hr 消失。
  {
    const before = await page.evaluate(() => window.__probe.blockTags());
    let after = before;
    for (let i = 0; i < 3 && after.includes('HR'); i++) {
      await page.click('.m-dock button[aria-label="撤销"]');
      await page.waitForTimeout(120);
      after = await page.evaluate(() => window.__probe.blockTags());
    }
    record('④a 撤销分割线', !after.includes('HR') && after.length < before.length,
      `before=${before.length} after=${after.length} tags=${after.join(',')}`);
    record('④b 撤销后无错', errors.length === 0, `errors=${errors.length}`);
  }

  // ⑤ 块上下移：自建场景（插 H1+H2——插入命令自带 TextSelection 光标定位，
  // 绕开「空块点击→PM 自动补段」干扰；H1/H2 异 tag 使 swap 可断言）
  {
    // 自建两块
    await page.click('.m-dock button[aria-label="插入块"]');
    await page.waitForTimeout(200);
    await page.click('.m-grid button[aria-label="标题一"]');
    await page.waitForTimeout(80);
    await page.click('.m-dock button[aria-label="插入块"]');
    await page.waitForTimeout(200);
    await page.click('.m-grid button[aria-label="标题二"]');
    await page.waitForTimeout(80);
    const before = await page.evaluate(() => window.__probe.blockTags());
    // ⑤a 末块（H2）下移=禁用态
    const downDisabled = await page.evaluate(() => {
      const b = document.querySelector('.m-dock button[aria-label="块下移"]');
      return b ? b.disabled : null;
    });
    const afterDown = await page.evaluate(() => window.__probe.blockTags());
    record('⑤a 末块下移禁用', downDisabled === true && JSON.stringify(before) === JSON.stringify(afterDown),
      `disabled=${downDisabled} tags=${afterDown.join(',')}`);
    // ⑤b 上移（末块 H2 ↔ 前块 H1 swap——尾 2 反转）
    await page.click('.m-dock button[aria-label="块上移"]');
    await page.waitForTimeout(120);
    const afterUp = await page.evaluate(() => window.__probe.blockTags());
    const swapped = afterUp.length === before.length
      && afterUp[afterUp.length - 2] === before[before.length - 1]
      && afterUp[afterUp.length - 1] === before[before.length - 2];
    record('⑤b 上移 swap', swapped,
      `tailBefore=${before.slice(-2).join(',')} tailAfter=${afterUp.slice(-2).join(',')}`);
  }

  // ⑥ 关面板（遮罩点击）
  {
    await page.click('.m-dock button[aria-label="插入块"]');
    await page.waitForTimeout(200);
    await page.click('.m-sheet-mask', { position: { x: 195, y: 60 } });
    await page.waitForTimeout(200);
    const open = await page.evaluate(() => window.__probe.panelOpen());
    record('⑥ 遮罩关面板', !open, 'm-sheet 无 open class');
  }

  // ⑦ errors === 0（双监听）
  record('⑦ 全程零错误', errors.length === 0, errors.length ? errors.slice(0, 3).join(' | ') : 'pageerror+console.error 均 0');

  const failed = results.filter((r) => !r.pass).length;
  console.log(`\n=== R19 验收: ${results.length - failed}/${results.length} PASS ===`);
  await browser.close();
  process.exit(failed > 0 ? 1 : 0);
})();
