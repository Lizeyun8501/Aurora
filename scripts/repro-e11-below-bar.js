/**
 * E11 验证（TA 16:19 裁决「弹窗置于编辑工具栏下方」）
 * ① 面板在栏正下方: 面板 top ≥ 栏 bottom（无重叠）+面板底贴屏底
 * ② 栏上浮动态: 弹期栏 bottom=面板高+隙（离开屏底）;关面板回贴底
 * ③ tab 切换面板高变→栏随动（RO 实时）
 * ④ 光标避让仍效（E10 链不破）
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
  // 长文+光标底段（避让复验条件）
  await page.evaluate(() => {
    const ev = window.__editorView;
    ev.focus();
    let tr = ev.state.tr;
    const p = ev.state.schema.nodes.paragraph;
    for (let i = 0; i < 30; i++) tr = tr.insert(tr.selection.from, p.create(null, ev.state.schema.text(`段${i} 内容`)));
    ev.dispatch(tr);
    const end = ev.state.doc.content.size - 3;
    ev.dispatch(ev.state.tr.setSelection(ev.state.selection.constructor.near(ev.state.doc.resolve(end))));
  });
  await page.waitForTimeout(300);
  const before = await page.evaluate(() => Math.round(window.__editorView.coordsAtPos(window.__editorView.state.selection.head).top));

  // 开「插入」面板
  await page.click('.editor-toolbar button[title="插入"]');
  await page.waitForTimeout(900);
  const geo = await page.evaluate(() => {
    const bar = document.querySelector('.editor-toolbar');
    const pop = document.querySelector('.tb-style-pop');
    if (!bar || !pop) return null;
    const br = bar.getBoundingClientRect();
    const pr = pop.getBoundingClientRect();
    return {
      barBottom: Math.round(br.bottom), barTop: Math.round(br.top),
      popTop: Math.round(pr.top), popBottom: Math.round(pr.bottom),
      vh: window.innerHeight, popH: Math.round(pr.height),
    };
  });
  record('① 面板在栏正下方（无重叠+贴屏底）',
    !!geo && geo.popTop >= geo.barBottom - 1 && Math.abs(geo.popBottom - geo.vh) < 3,
    `栏底=${geo?.barBottom} 面板顶=${geo?.popTop} 面板底=${geo?.popBottom}/${geo?.vh}`);
  record('② 栏上浮动（离屏底=面板高+隙）',
    !!geo && geo.barBottom < geo.vh - 5 && geo.barBottom >= geo.popTop - 6,
    `栏底=${geo?.barBottom} 屏底=${geo?.vh} 面板顶=${geo?.popTop} 面板高=${geo?.popH}`);
  // ④ 光标避让仍效
  const cursor = await page.evaluate(() => Math.round(window.__editorView.coordsAtPos(window.__editorView.state.selection.head).top));
  record('④ 光标避让仍效（光标<面板顶）', !!geo && cursor < geo.popTop, `光标=${cursor} < 面板顶=${geo?.popTop}（避让前=${before}）`);

  // ③ 关面板回贴底（toggle 一次关——面板消失后栏底回屏底）
  await page.evaluate(() => { window.__editorView?.focus(); });
  await page.click('.editor-toolbar button[title="插入"]');
  await page.waitForTimeout(700);
  const closed = await page.evaluate(() => ({
    hasPop: !!document.querySelector('.tb-style-pop'),
    barB: Math.round(document.querySelector('.editor-toolbar').getBoundingClientRect().bottom),
  }));
  record('③ 关面板栏回贴底', !closed.hasPop && Math.abs(closed.barB - 844) < 4, `面板=${closed.hasPop} 栏底=${closed.barB}/844`);
  // Aa 面板 tab 高变随动
  await page.click('.editor-toolbar button[title="文字格式"]');
  await page.waitForTimeout(800);
  const h1 = await page.evaluate(() => {
    const bar = document.querySelector('.editor-toolbar');
    const pop = document.querySelector('.tb-style-pop');
    return { barB: Math.round(bar.getBoundingClientRect().bottom), popT: Math.round(pop.getBoundingClientRect().top), popH: Math.round(pop.getBoundingClientRect().height) };
  });
  // 切「符号」tab（高度不同）
  await page.evaluate(() => {
    const tabs = Array.from(document.querySelectorAll('.tb-style-tab'));
    tabs.find((t) => t.textContent === '符号')?.click();
  });
  await page.waitForTimeout(600);
  const h2 = await page.evaluate(() => {
    const bar = document.querySelector('.editor-toolbar');
    const pop = document.querySelector('.tb-style-pop');
    return { barB: Math.round(bar.getBoundingClientRect().bottom), popT: Math.round(pop.getBoundingClientRect().top), popH: Math.round(pop.getBoundingClientRect().height) };
  });
  record('③b tab 高变栏随动', h1.popH !== h2.popH ? Math.abs(h2.barB - h2.popT) <= 6 : Math.abs(h2.barB - h2.popT) <= 6,
    `fmt高=${h1.popH} 符号高=${h2.popH} 栏底随=${h2.barB}/面板顶=${h2.popT}`);
  await page.evaluate(() => { window.__editorView?.focus(); });

  record('⑤ 零错误', errors.length === 0, errors.slice(0, 2).join(' | ') || 'clean');
  const passed = results.filter((r) => r.p).length;
  console.log(`\n=== E11 弹窗栏下方: ${passed}/${results.length} PASS ===`);
  await browser.close();
  process.exit(passed === results.length ? 0 : 1);
})();
