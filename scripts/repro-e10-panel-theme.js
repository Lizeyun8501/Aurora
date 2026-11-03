/**
 * E10 双卡验证（TA 14:03）
 * ① 光标避让: 弹面板时光标行滚入面板上方安全区（视觉 top ≤ 55vh）
 * ② 面板自栏生长: transform-origin bottom + 动画（scaleY）
 * ③ 主题三态: system→matchMedia 解析 data-theme;切深色/浅色/系统互转
 * ④ 系统实时随: 模拟 prefers-color-scheme 变→system 模式即时切
 */
const { chromium } = require('/home/z/.npm-global/lib/node_modules/playwright');
const BASE = 'file:///home/z/my-project/repos/Aurora/apps/mobile/dist/index.html';

(async () => {
  const browser = await chromium.launch();
  const ctx = await browser.newContext({ viewport: { width: 390, height: 844 }, colorScheme: 'light', hasTouch: true });
  const page = await ctx.newPage();
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
  // 造长文（光标在中下）: 30 段
  await page.evaluate(() => {
    const ev = window.__editorView;
    ev.focus();
    let tr = ev.state.tr;
    const p = ev.state.schema.nodes.paragraph;
    for (let i = 0; i < 30; i++) tr = tr.insert(tr.selection.from, p.create(null, ev.state.schema.text(`段落${i} 内容内容内容`)));
    ev.dispatch(tr);
    // 光标置末段中
    const end = ev.state.doc.content.size - 3;
    ev.dispatch(ev.state.tr.setSelection(ev.state.selection.constructor.near(ev.state.doc.resolve(end))));
    ev.focus();
  });
  await page.waitForTimeout(400);
  // 弹面板前光标视觉位
  const before = await page.evaluate(() => {
    const ev = window.__editorView;
    const c = ev.coordsAtPos(ev.state.selection.head);
    return { top: Math.round(c.top), vh: window.innerHeight };
  });
  // 开「插入」面板（触发 hideKbdForPanel→光标避让）
  await page.click('.editor-toolbar button[title="插入"]');
  await page.waitForTimeout(1100);
  const after = await page.evaluate(() => {
    const ev = window.__editorView;
    const c = ev.coordsAtPos(ev.state.selection.head);
    const pop = document.querySelector('.tb-style-pop');
    const pr = pop ? pop.getBoundingClientRect() : null;
    const cs = pop ? getComputedStyle(pop) : null;
    const oy = cs ? Number((cs.transformOrigin.split(' ')[1] ?? '0').replace('px', '')) : 0;
    return {
      cursorTop: Math.round(c.top), vh: window.innerHeight,
      popTop: pr ? Math.round(pr.top) : null,
      popH: pr ? Math.round(pr.height) : 0,
      originY: oy, origin: cs?.transformOrigin ?? '',
    };
  });
  // ① 光标避让: 光标 top 须 < 面板 top（在面板上方）——光标原在底半区被滚上来
  record('① 光标避让（光标在面板上方可视区）',
    after.cursorTop < (after.popTop ?? after.vh) && after.popTop !== null,
    `光标 top=${after.cursorTop} 面板 top=${after.popTop}（避让前 top=${before.top}/${before.vh}）`);
  // ② 生长动画锚
  record('② 自栏位生长锚（origin=顶边——E11 面板在栏下方）', after.popH > 0 && after.originY < 3, `originY=${after.originY}（origin=${after.origin}）`);
  await page.evaluate(() => { window.__editorView?.focus(); });

  // ③ 主题三态: 返回→菜单抽屉→设置
  await page.click('.editor-bar button[aria-label="返回"]');
  await page.waitForTimeout(800);
  await page.click('.app-bar button[aria-label="菜单"]');
  await page.waitForTimeout(600);
  await page.click('.d-item:has-text("设置")');
  await page.waitForTimeout(900);
  const themeSeg = await page.evaluate(() => {
    const btns = Array.from(document.querySelectorAll('.settings-card .nt-seg'));
    return { count: btns.length, labels: btns.map((b) => b.textContent) };
  });
  record('③ 主题三段（浅/深/系统）', themeSeg.count === 3, `段=${themeSeg.labels.join('|')}`);
  // 点「跟随系统」
  await page.evaluate(() => {
    const btns = Array.from(document.querySelectorAll('.settings-card .nt-seg'));
    btns.find((b) => b.textContent?.includes('跟随系统'))?.click();
  });
  await page.waitForTimeout(400);
  const sysResolve = await page.evaluate(() => document.documentElement.dataset.theme);
  record('③b system→matchMedia 解析（light 语境=light）', sysResolve === 'light', `data-theme=${sysResolve}`);
  // ④ 系统实时随: colorScheme 切 dark
  await page.emulateMedia({ colorScheme: 'dark' });
  await page.waitForTimeout(600);
  const sysFollow = await page.evaluate(() => document.documentElement.dataset.theme);
  record('④ 系统切换实时随（dark 语境→dark）', sysFollow === 'dark', `data-theme=${sysFollow}`);
  // 深色持久: 切深色档→data-theme dark;回系统→回 light
  await page.evaluate(() => {
    const btns = Array.from(document.querySelectorAll('.settings-card .nt-seg'));
    btns.find((b) => b.textContent?.includes('深色'))?.click();
  });
  await page.waitForTimeout(300);
  const forced = await page.evaluate(() => document.documentElement.dataset.theme);
  await page.emulateMedia({ colorScheme: 'light' });
  await page.waitForTimeout(400);
  const forcedStay = await page.evaluate(() => document.documentElement.dataset.theme);
  record('③c 深色显式档不受系统反转', forced === 'dark' && forcedStay === 'dark', `dark时=${forced} 回light语境=${forcedStay}`);

  record('⑤ 零错误', errors.length === 0, errors.slice(0, 2).join(' | ') || 'clean');
  const passed = results.filter((r) => r.p).length;
  console.log(`\n=== E10 面板避让+主题三态: ${passed}/${results.length} PASS ===`);
  await browser.close();
  process.exit(passed === results.length ? 0 : 1);
})();
