/**
 * E26 验证（TA 22:13 四项）
 * ① 块类型面板 14 项 4 列×4 行全显（无横溢——scrollWidth<=clientWidth）
 * ② </> 行内代码 title 说明
 * ③ 字号面板数字级（12-30 七档）
 * ④ 工具栏钮 :active 瞬态无背景（computedStyle 验证）
 */
const { chromium } = require('/home/z/.npm-global/lib/node_modules/playwright');
const BASE = 'file:///home/z/my-project/Aurora/apps/mobile/dist/index.html';

(async () => {
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 390, height: 844 }, hasTouch: true });
  const results = [];
  const record = (n, p, d) => { results.push({ n, p }); console.log(`${p ? 'PASS' : 'FAIL'} ${n}: ${d}`); };
  const errors = [];
  page.on('pageerror', (e) => errors.push(String(e).slice(0, 100)));

  await page.goto(BASE);
  await page.waitForTimeout(1600);
  await page.click('.tab-bar .tab-item:has-text("知识库")');
  await page.waitForTimeout(700);
  await page.click('.nt-arrow');
  await page.waitForTimeout(400);
  await page.click('.nt-add');
  await page.waitForTimeout(900);
  await page.click('.nt-row:has-text("未命名")');
  await page.waitForSelector('.ProseMirror', { timeout: 10000 });
  await page.click('.ProseMirror');
  await page.keyboard.type('E26 验证', { delay: 10 });
  await page.waitForTimeout(400);

  // 开块类型面板
  await page.click('button[title="块类型"]');
  await page.waitForTimeout(600);

  // ① 面板无横溢+4 列+14 项全显
  const grid = await page.evaluate(() => {
    const g = document.querySelector('.tb-style-grid-btns');
    if (!g) return null;
    const cs = getComputedStyle(g);
    const btns = g.querySelectorAll('.tb-style-btn');
    return { cols: cs.gridTemplateColumns.split(' ').length, count: btns.length, noHOverflow: g.scrollWidth <= g.clientWidth + 1, popNoH: document.querySelector('.tb-style-pop')?.scrollWidth <= (document.querySelector('.tb-style-pop')?.clientWidth ?? 0) + 1 };
  });
  record('① 块面板 4 列×14 项全显无横溢', grid && grid.cols === 4 && grid.count === 14 && grid.noHOverflow && grid.popNoH, JSON.stringify(grid));
  const lastVisible = await page.evaluate(() => {
    const btns = Array.from(document.querySelectorAll('.tb-style-grid-btns .tb-style-btn'));
    const last = btns[btns.length - 1];
    const r = last.getBoundingClientRect();
    return r.right <= window.innerWidth && r.width > 0;
  });
  record('①b 末项（思维导图）完全可见', lastVisible === true, `末项 right<=vw=${lastVisible}`);

  // ②+③: 开格式面板（Aa——block 面板先关）
  await page.click('button[title="块类型"]'); // toggle 关
  await page.waitForTimeout(400);
  await page.click('button[title*="格式"], button[title="Aa"], button[title="文字"]');
  await page.waitForTimeout(500);

  // ② </> title
  const codeTitle = await page.evaluate(() => {
    const f = document.querySelector('.tb-fmt-row button[title*="行内代码"]');
    return { fmt: !!f };
  });
  record('② 行内代码钮 title 说明（</>）', codeTitle.fmt, JSON.stringify(codeTitle));

  // ③ 字号数字级
  const sizes = null;
  const sizePanel = await page.evaluate(() => {
    const g = document.querySelector('[aria-label="字号档位"]');
    if (!g) return null;
    const labels = Array.from(g.querySelectorAll('.tb-size')).map((b) => b.textContent?.trim());
    return { labels };
  });
  const ok3 = sizePanel && sizePanel.labels.length === 7 && sizePanel.labels.every((l) => /^\d+$/.test(l)) && sizePanel.labels[0] === '12' && sizePanel.labels[6] === '30';
  record('③ 字号数字级 12-30 七档', ok3, JSON.stringify(sizePanel?.labels));

  // ④ :active 瞬态无背景
  const activeBg = await page.evaluate(() => {
    // 模拟 :active——加类探 computedStyle（active 伪类不易程序化——直接验 CSS 规则文本）
    for (const sheet of Array.from(document.styleSheets)) {
      try {
        for (const rule of Array.from(sheet.cssRules)) {
          if (rule.selectorText === '.tb-btn:active') return { bg: rule.style.background || rule.style.backgroundColor };
        }
      } catch { /* cross-origin skip */ }
    }
    return null;
  });
  record('④ 工具栏 :active 瞬态背景清', activeBg && (activeBg.bg === 'transparent' || activeBg.bg.includes('transparent')), JSON.stringify(activeBg));

  record('⑤ 零错误', errors.length === 0, errors.slice(0, 2).join('|') || 'clean');
  const passed = results.filter((r) => r.p).length;
  console.log(`\n=== E26 工具栏四项: ${passed}/${results.length} PASS ===`);
  await browser.close();
  process.exit(passed === results.length ? 0 : 1);
})();
