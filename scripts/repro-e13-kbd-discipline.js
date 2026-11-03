/**
 * E13 验证（TA 17:56: 面板内容点击输入法不跳;点工具栏按钮/编辑页→面板消+输入法出）
 * ① 粘期命令应用不抢焦点（hasFocus=false——输入法不弹）
 * ② 效果仍生效（dispatch 不需焦点）
 * ③ 编辑区点击→面板关+focus（输入法出）
 * ④ 栏钮 toggle 关→focus（输入法出）
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
  await page.keyboard.type('红字重点蓝字', { delay: 30 });
  await page.waitForTimeout(300);
  await page.evaluate(() => {
    const ev = window.__editorView;
    ev.dispatch(ev.state.tr.setSelection(ev.state.selection.constructor.create(ev.state.doc, 1, 3)));
  });
  // 开面板（hideKbdForPanel 已 blur——焦点出）
  await page.click('.editor-toolbar button[title="文字格式"]');
  await page.waitForTimeout(700);
  const openState = await page.evaluate(() => ({
    focused: window.__editorView.hasFocus(),
    hasPop: !!document.querySelector('.tb-style-pop'),
  }));
  record('前置 开面板失焦（kbd 让位）', !openState.focused && openState.hasPop, `focused=${openState.focused}`);

  // ① 点「加粗」——粘性+不抢焦点
  await page.click('.tb-fmt-row button[title="加粗"]');
  await page.waitForTimeout(400);
  const afterBold = await page.evaluate(() => {
    const ev = window.__editorView;
    const marks = (ev.state.doc.nodeAt(1)?.marks ?? []).map((m) => m.type.name);
    return { focused: ev.hasFocus(), hasPop: !!document.querySelector('.tb-style-pop'), marks };
  });
  record('① 点面板内容输入法不跳（不抢焦点）', !afterBold.focused && afterBold.hasPop,
    `focused=${afterBold.focused} 面板=${afterBold.hasPop}`);
  record('② 格式仍生效（dispatch 无需焦点）', afterBold.marks.includes('strong'), `marks=${afterBold.marks}`);

  // ③ 编辑区点击→面板关+focus（输入法出）
  await page.click('.ProseMirror', { position: { x: 30, y: 18 } });
  await page.waitForTimeout(500);
  const editClick = await page.evaluate(() => ({
    hasPop: !!document.querySelector('.tb-style-pop'),
    focused: window.__editorView.hasFocus(),
  }));
  record('③ 编辑区点击→面板关+输入法出', !editClick.hasPop && editClick.focused, `面板=${editClick.hasPop} focused=${editClick.focused}`);

  // ④ 栏钮 toggle 关→focus（③ 后焦点本在——真机流: 编辑中点栏钮（blur+wasActive=true）→重按（restore→focus）
  await page.click('.editor-toolbar button[title="插入"]');
  await page.waitForTimeout(700);
  await page.click('.editor-toolbar button[title="插入"]');
  await page.waitForTimeout(600);
  const barToggle = await page.evaluate(() => ({
    hasPop: !!document.querySelector('.tb-style-pop'),
    focused: window.__editorView.hasFocus(),
  }));
  record('④ 栏钮重按关→面板消+输入法出', !barToggle.hasPop && barToggle.focused, `面板=${barToggle.hasPop} focused=${barToggle.focused}`);

  // ⑤ 粘期插入类（HR）也不弹
  await page.click('.editor-toolbar button[title="插入"]');
  await page.waitForTimeout(700);
  const hrBtn = page.locator('.tb-style-pop button[title="分割线"]');
  if (await hrBtn.count()) {
    await hrBtn.click();
    await page.waitForTimeout(400);
    const hrState = await page.evaluate(() => {
      const ev = window.__editorView;
      const types = [];
      ev.state.doc.forEach((n) => types.push(n.type.name));
      return { focused: ev.hasFocus(), hasHr: types.includes('horizontal_rule'), hasPop: !!document.querySelector('.tb-style-pop') };
    });
    record('⑤ 插入类不弹输入法（HR 粘性生效）', !hrState.focused && hrState.hasHr && hrState.hasPop,
      `focused=${hrState.focused} hr=${hrState.hasHr} 面板=${hrState.hasPop}`);
  } else {
    record('⑤ 插入类不弹输入法（HR 粘性生效）', false, 'HR 钮未找到');
  }
  await page.evaluate(() => { window.__editorView?.focus(); });

  record('⑥ 零错误', errors.length === 0, errors.slice(0, 2).join(' | ') || 'clean');
  const passed = results.filter((r) => r.p).length;
  console.log(`\n=== E13 输入法纪律: ${passed}/${results.length} PASS ===`);
  await browser.close();
  process.exit(passed === results.length ? 0 : 1);
})();
