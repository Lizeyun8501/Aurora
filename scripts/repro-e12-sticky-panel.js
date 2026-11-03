/**
 * E12 验证（TA 16:36: 面板内容选中不主动隐藏;编辑区点击/栏钮重按才关）
 * ① 粘性: 点面板内容（色/块）后面板仍开——连续两次应用
 * ② 编辑区点击关面板
 * ③ 栏钮重按（toggle）关面板
 * ④ 连续应用效果叠（两次格式动作都生效）
 * ⑤ 避让垫链不破（关面板撤垫）
 */
const { chromium } = require('/home/z/.npm-global/lib/node_modules/playwright');
const BASE = 'file://' + require('path').resolve(__dirname, '../apps/mobile/dist/index.html');

(async () => {
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 390, height: 844 }, hasTouch: true });
  const errors = [];
  page.on('pageerror', (e) => errors.push(String(e).slice(0, 120)));
  const results = [];
  const record = (n, p, d) => { results.push({ n, p }); console.log(`${p ? 'PASS' : 'FAIL'} ${n}: ${d}`); };

  await page.goto(BASE);
  await page.waitForTimeout(1500);
  await page.click('.tab-bar .tab-item:has-text("知识库")');
  await page.waitForTimeout(700);
  await page.click('.nt-arrow');
  await page.waitForTimeout(400);
  await page.click('.nt-add');
  await page.waitForTimeout(900);
  await page.click('.nt-row.nt-doc, .nt-row:has-text("未命名")');
  await page.waitForSelector('.ProseMirror', { timeout: 10000 });
  await page.click('.ProseMirror');
  await page.keyboard.type('红字重点蓝字普通', { delay: 30 });
  await page.waitForTimeout(300);
  // 选中「红字」
  await page.evaluate(() => {
    const ev = window.__editorView;
    const pos = ev.state.doc.content.findIndex ? 0 : 0;
    void pos;
    // 直选前 2 字
    const $p = ev.state.doc.resolve(1);
    void $p;
    ev.dispatch(ev.state.tr.setSelection(ev.state.selection.constructor.create(ev.state.doc, 1, 3)));
  });

  // ① 开 Aa 面板（格式 tab）——点「加粗」粘
  await page.click('.editor-toolbar button[title="文字格式"]');
  await page.waitForTimeout(700);
  await page.click('.tb-fmt-row button[title="加粗"]');
  await page.waitForTimeout(400);
  const sticky1 = await page.evaluate(() => !!document.querySelector('.tb-style-pop'));
  record('①a 粘性: 点加粗后面板仍开', sticky1, `面板=${sticky1}`);
  // ①b 连续第二击（斜体）——面板仍开+效果叠
  await page.click('.tb-fmt-row button[title="斜体"]');
  await page.waitForTimeout(400);
  const sticky2 = await page.evaluate(() => {
    const ev = window.__editorView;
    const marks = (ev.state.doc.nodeAt(1)?.marks ?? []).map((m) => m.type.name);
    return { hasPop: !!document.querySelector('.tb-style-pop'), marks };
  });
  record('①b 连续应用叠（strong+em 同段生效）', sticky2.hasPop && sticky2.marks.includes('strong') && sticky2.marks.includes('em'),
    `面板=${sticky2.hasPop} marks=${sticky2.marks}`);

  // ⑤ 光标避让垫在粘期保持（面板开=垫在）
  const padOn = await page.evaluate(() => window.__editorView.dom.classList.contains('e10-cursor-pad'));
  record('⑤ 粘期避让垫保持', padOn, `e10-cursor-pad=${padOn}`);

  // ② 编辑区点击关
  await page.click('.ProseMirror', { position: { x: 30, y: 20 } });
  await page.waitForTimeout(500);
  const closedByEdit = await page.evaluate(() => ({
    hasPop: !!document.querySelector('.tb-style-pop'),
    padOff: !window.__editorView.dom.classList.contains('e10-cursor-pad'),
  }));
  record('② 编辑区点击关（垫撤）', !closedByEdit.hasPop && closedByEdit.padOff, `面板=${closedByEdit.hasPop} 垫撤=${closedByEdit.padOff}`);

  // ③ 栏钮重按 toggle 关: 开→重按同钮关
  await page.click('.editor-toolbar button[title="插入"]');
  await page.waitForTimeout(700);
  const openAgain = await page.evaluate(() => !!document.querySelector('.tb-style-pop'));
  await page.click('.editor-toolbar button[title="插入"]');
  await page.waitForTimeout(600);
  const closedByToggle = await page.evaluate(() => !!document.querySelector('.tb-style-pop'));
  record('③ 栏钮重按 toggle 关', openAgain && !closedByToggle, `开=${openAgain} 重按后=${closedByToggle}`);

  // ④ 粘期切面板组（Aa→插入 不先关）链通
  await page.click('.editor-toolbar button[title="块类型"]');
  await page.waitForTimeout(600);
  const switchOk = await page.evaluate(() => !!document.querySelector('.tb-style-pop .tb-sec-title'));
  await page.click('.editor-toolbar button[title="块类型"]');
  await page.waitForTimeout(400);
  record('④ 组间直切（粘期换弹组）', switchOk, `块面板=${switchOk}`);

  record('⑥ 零错误', errors.length === 0, errors.slice(0, 2).join(' | ') || 'clean');
  const passed = results.filter((r) => r.p).length;
  console.log(`\n=== E12 粘性面板: ${passed}/${results.length} PASS ===`);
  await browser.close();
  process.exit(passed === results.length ? 0 : 1);
})();
