// 最小复现: 进编辑器→type→点 Aa→查面板 DOM+React 态
const { chromium } = require('/home/z/.npm-global/lib/node_modules/playwright');
(async () => {
  const browser = await chromium.launch();
  const page = await browser.newPage();
  const errs = [];
  page.on('console', (m) => { if (m.type() === 'error') errs.push(m.text().slice(0, 200)); });
  page.on('pageerror', (e) => errs.push(String(e).slice(0, 200)));
  await page.goto('file:///home/z/my-project/Aurora/apps/mobile/dist/index.html');
  await page.waitForTimeout(1200);
  await page.click('.tab-bar .tab-item:has-text("知识库")');
  await page.waitForTimeout(700);
  await page.click('.nt-arrow');
  await page.waitForTimeout(400);
  await page.click('.nt-add');
  await page.waitForTimeout(900);
  await page.click('.nt-row.nt-doc, .nt-row:has-text("未命名")');
  await page.waitForSelector('.ProseMirror', { timeout: 10000 });
  await page.click('.ProseMirror');
  await page.keyboard.type('测试文字', { delay: 40 });
  await page.waitForTimeout(300);
  const hasFocus = await page.evaluate(() => window.__editorView?.hasFocus());
  await page.click('.editor-toolbar button[title="文字格式"]');
  await page.waitForTimeout(400);
  const st = await page.evaluate(() => {
    const panel = document.querySelector('.tb-style-pop');
    const bar = document.querySelector('.editor-toolbar');
    const pr = panel?.getBoundingClientRect();
    const br = bar?.getBoundingClientRect();
    return {
      panel: !!panel,
      kbd: document.documentElement.style.getPropertyValue('--kbd-inset'),
      focusAfter: window.__editorView?.hasFocus(),
      btnText: document.querySelector('.editor-toolbar button[title="文字格式"]')?.textContent,
      pr: pr ? { top: pr.top, bottom: pr.bottom, left: pr.left, right: pr.right } : null,
      br: br ? { top: br.top, bottom: br.bottom } : null,
    };
  });
  const twin = await page.evaluate(() => ({
    toolbars: document.querySelectorAll('.editor-toolbar').length,
    pops: document.querySelectorAll('.tb-style-pop').length,
    wraps: document.querySelectorAll('.tb-pop-wrap').length,
    // 面板父节点即 toolbar? portal 后应为 body
    popParent: document.querySelector('.tb-style-pop')?.parentElement?.className || null,
    // Aa 钮同 title 双生?
    aaBtns: document.querySelectorAll('.editor-toolbar button[title="文字格式"]').length,
  }));
  console.log('TWIN:', JSON.stringify(twin));
  // E4 遮挡终检: 色板首 swatch 命中谁
  const swatchHit = await page.evaluate(() => {
    const sw = document.querySelector('.tb-fmt-panel .tb-swatch');
    if (!sw) return null;
    const r = sw.getBoundingClientRect();
    const el = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
    const bar = document.querySelector('.editor-toolbar');
    const pr = document.querySelector('.tb-style-pop').getBoundingClientRect();
    const br = bar.getBoundingClientRect();
    return {
      swRect: { top: r.top, bottom: r.bottom },
      hit: el ? el.tagName + '.' + String(el.className).split(' ')[0] : null,
      hitIsSw: el === sw,
      panelRect: { top: pr.top, bottom: pr.bottom },
      barRect: { top: br.top, bottom: br.bottom },
      overlapTop: Math.max(0, pr.bottom - br.top),  // 面板底伸入栏顶多少
    };
  });
  console.log('SW:', JSON.stringify(swatchHit));
  const domChain = await page.evaluate(() => {
    const btn = document.querySelector('.editor-toolbar button[title="文字格式"]');
    const pop = document.querySelector('.tb-style-pop');
    const tbPop = document.querySelector('.tb-pop');
    // 链 1: Aa 钮 → 根
    let chain1 = [];
    let n = btn;
    while (n && n !== document.body) { chain1.push(n.tagName + '.' + String(n.className).split(' ')[0]); n = n.parentElement; }
    chain1.push('BODY');
    // 链 2: 栏内命中的 tb-pop → 根
    const r = btn.getBoundingClientRect();
    const el = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
    let chain2 = [];
    n = el;
    while (n && n !== document.body) { chain2.push(n.tagName + '.' + String(n.className).split(' ')[0]); n = n.parentElement; }
    // 链 3: 面板 tb-pop → 根
    let chain3 = [];
    n = tbPop;
    while (n && n !== document.body) { chain3.push(n.tagName + '.' + String(n.className).split(' ')[0]); n = n.parentElement; }
    return { btnChain: chain1.join(' > '), hitChain: chain2.join(' > '), popChain: chain3.join(' > '), same: el === tbPop };
  });
  console.log('CHAIN:', JSON.stringify(domChain, null, 1));
  const hit = await page.evaluate(() => {
    const btn = document.querySelector('.editor-toolbar button[title="文字格式"]');
    const r = btn.getBoundingClientRect();
    const cx = r.left + r.width / 2, cy = r.top + r.height / 2;
    const el = document.elementFromPoint(cx, cy);
    const bar = document.querySelector('.editor-toolbar');
    const pop = document.querySelector('.tb-style-pop');
    return {
      hitTag: el ? el.tagName + '.' + String(el.className).split(' ')[0] : null,
      hitInBar: !!(el && bar.contains(el)),
      hitInPop: !!(el && pop && pop.contains(el)),
      barZ: getComputedStyle(bar).zIndex,
      popZ: pop ? getComputedStyle(pop).zIndex : null,
      barRect: { top: bar.getBoundingClientRect().top, bottom: bar.getBoundingClientRect().bottom },
      btnRect: { top: r.top, bottom: r.bottom },
    };
  });
  // ⑪ 中段复现: toggle 关→focus→toggle 开→hasFocus?
  await page.click('.editor-toolbar button[title="文字格式"]');
  await page.waitForTimeout(250);
  await page.evaluate(() => { window.__editorView?.focus(); });
  await page.waitForTimeout(250);
  const fm = await page.evaluate(() => window.__editorView?.hasFocus());
  await page.click('.editor-toolbar button[title="文字格式"]');
  await page.waitForTimeout(300);
  const fa = await page.evaluate(() => window.__editorView?.hasFocus());
  console.log('MID: focusedMid=' + fm + ' focusAfterReopen=' + fa);
  console.log('DBG:', JSON.stringify({ hasFocus, ...st, ...hit, errs: errs.slice(0, 3) }));
  await browser.close();
})();
