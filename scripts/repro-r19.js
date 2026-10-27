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
  {
    const cases = [
      { label: '标题一', tag: 'H1', detail: 'h1' },
      { label: '标题二', tag: 'H2', detail: 'h2' },
      { label: '无序列表', tag: 'UL', detail: 'ul' },
      { label: '有序列表', tag: 'OL', detail: 'ol' },
      { label: '任务列表', tag: 'DIV', detail: 'task-block div' },
      { label: '引述块', tag: 'BLOCKQUOTE', detail: 'blockquote' },
      { label: '代码块', tag: 'PRE', detail: 'pre' },
      { label: '表格', tag: 'TABLE', detail: 'table' },
      { label: '分割线', tag: 'HR', detail: 'hr' },
    ];
    for (const c of cases) {
      // 打开面板（上次插入后已关）
      await page.click('.m-dock button[aria-label="插入块"]');
      await page.waitForTimeout(200);
      await page.click(`.m-grid button[aria-label="${c.label}"]`);
      await page.waitForTimeout(80);
      const tags = await page.evaluate(() => window.__probe.blockTags());
      record(`③ 插入${c.label}`, tags.includes(c.tag), `tags=${tags.join(',')}`);
    }
  }

  // ④ 撤销：末次插入（分割线）回退
  {
    const before = await page.evaluate(() => window.__probe.blockTags());
    await page.click('.m-dock button[aria-label="撤销"]');
    await page.waitForTimeout(120);
    const after = await page.evaluate(() => window.__probe.blockTags());
    record('④a 撤销分割线', before.length - after.length === 1 && !after.includes('HR'),
      `before=${before.length} after=${after.length} tags=${after.join(',')}`);
    record('④b 撤销后无错', errors.length === 0, `errors=${errors.length}`);
  }

  // ⑤ 块上下移：多块场景 swap（现有 8 顶层块）
  {
    const before = await page.evaluate(() => window.__probe.blockTags());
    // 光标进最后一块（点编辑器底部）
    const paras = await page.$$('.rich-editor-host .ProseMirror > *');
    await paras[paras.length - 1].click();
    await page.waitForTimeout(80);
    // 下移（末块 → 边界禁用态，无操作）
    await page.click('.m-dock button[aria-label="块下移"]');
    await page.waitForTimeout(120);
    const afterDown = await page.evaluate(() => window.__probe.blockTags());
    record('⑤a 末块下移禁用', JSON.stringify(before) === JSON.stringify(afterDown), 'tags 不变');
    // 上移（末块 ↔ 前块 swap）
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
