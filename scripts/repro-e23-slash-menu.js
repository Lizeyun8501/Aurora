/**
 * E23 验证（TA 22:25: 思源编辑功能引入——斜杠菜单）
 * ① '/' 空行触发→菜单开（10 项）
 * ② query 过滤（/标 → 标题系）
 * ③ 点项→语法删+块插入（H1）
 * ④ Esc 关
 * ⑤ 段中无空格 '/' 不误触
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
  await page.waitForTimeout(1600);
  await page.click('.tab-bar .tab-item:has-text("知识库")');
  await page.waitForTimeout(700);
  await page.click('.nt-arrow');
  await page.waitForTimeout(400);
  await page.click('.nt-add');
  await page.waitForTimeout(900);
  await page.click('.nt-row.nt-doc, .nt-row:has-text("未命名")');
  await page.waitForSelector('.ProseMirror', { timeout: 10000 });
  await page.click('.ProseMirror');
  await page.waitForTimeout(400);

  // ① 空行 '/' 触发
  await page.keyboard.type('abc', { delay: 20 });
  await page.keyboard.press('Enter');
  await page.waitForTimeout(300);
  await page.keyboard.press('/');
  await page.waitForTimeout(600);
  const menu1 = await page.evaluate(() => ({
    open: !!document.querySelector('.slash-menu'),
    cells: document.querySelectorAll('.slash-cell').length,
    title: document.querySelector('.slash-menu .tb-sec-title')?.textContent,
  }));
  record('① / 空行触发菜单（18 项 E24 扩）', menu1.open && menu1.cells === 18, `开=${menu1.open} 项=${menu1.cells}`);

  // ② 过滤（输入 标）
  await page.keyboard.type('标', { delay: 30 });
  await page.waitForTimeout(600);
  const menu2 = await page.evaluate(() => ({
    cells: document.querySelectorAll('.slash-cell').length,
    labels: Array.from(document.querySelectorAll('.slash-label')).map((l) => l.textContent),
    title: document.querySelector('.slash-menu .tb-sec-title')?.textContent,
  }));
  record('② query 过滤（标→6 标题 E24 扩）', menu2.cells === 6 && menu2.labels.every((l) => l?.includes('标题')), `项=${menu2.cells} ${menu2.labels.join('|')}`);

  // ③ 点 H1→插入
  await page.evaluate(() => {
    const cells = Array.from(document.querySelectorAll('.slash-cell'));
    cells.find((c) => c.textContent?.includes('一级标题'))?.click();
  });
  await page.waitForTimeout(700);
  const doc = await page.evaluate(() => {
    const j = window.__editorView.state.doc.toJSON();
    return { first: j.content?.[0]?.type, second: j.content?.[1]?.type, text: j.content?.[1]?.content?.[0]?.text };
  });
  record('③ 点项插入 H1（语法删净）', doc.second === 'heading' && !doc.text?.includes('/') && !doc.text?.includes('标'),
    `块2=${doc.second} 文=${doc.text}`);

  // ④ Esc 关
  await page.keyboard.press('Enter');
  await page.waitForTimeout(300);
  await page.keyboard.press('/');
  await page.waitForTimeout(500);
  await page.keyboard.press('Escape');
  await page.waitForTimeout(400);
  const escClosed = await page.evaluate(() => {
    const j = window.__editorView.state.doc.toJSON();
    return { menu: !!document.querySelector('.slash-menu') };
  });
  record('④ Esc 关菜单', !escClosed.menu, `menu=${escClosed.menu}`);

  // ⑤ 段中无空格 / 不触（"a/b"）
  await page.keyboard.press('Enter');
  await page.keyboard.type('xy', { delay: 20 });
  await page.keyboard.press('/');
  await page.waitForTimeout(600);
  const mid = await page.evaluate(() => !!document.querySelector('.slash-menu'));
  record('⑤ 段中 / 不误触', !mid, `menu=${mid}`);

  record('⑥ 零错误', errors.length === 0, errors.slice(0, 2).join(' | ') || 'clean');
  const passed = results.filter((r) => r.p).length;
  console.log(`\n=== E23 斜杠菜单: ${passed}/${results.length} PASS ===`);
  await browser.close();
  process.exit(passed === results.length ? 0 : 1);
})();
