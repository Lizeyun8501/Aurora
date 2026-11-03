/**
 * E15 验证（TA 18:58: pdf 导出死机+word 未见文件）
 * ① Android UA 语境: pdf 走 share .html（无 window.open/print——死机根因拔除）
 * ② Android UA: pdf 后 App 活（返回/交互可用——无死窗）
 * ③ word: share reject（模拟组件拒绝）→blob 真兜底（download 事件到）——不静默假成功
 * ④ word: share AbortError（用户取消）→ok 静默（意图非失败）
 * ⑤ 桌面 UA: pdf 仍 window.open+print 链（语义保留）
 */
const { chromium } = require('/home/z/.npm-global/lib/node_modules/playwright');
const BASE = 'file:///home/z/my-project/Aurora/apps/mobile/dist/index.html';

(async () => {
  const browser = await chromium.launch();
  const results = [];
  const record = (n, p, d) => { results.push({ n, p }); console.log(`${p ? 'PASS' : 'FAIL'} ${n}: ${d}`); };
  const enterEditor = async (page) => {
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
    await page.keyboard.type('E15 pdf word 测试', { delay: 20 });
    await page.waitForTimeout(300);
  };
  const openMenuClick = async (page, text) => {
    await page.click('button[aria-label="更多"]');
    await page.waitForTimeout(400);
    await page.evaluate((t) => {
      const btn = Array.from(document.querySelectorAll('.menu-pop .menu-item')).find((b) => b.textContent?.includes(t));
      btn?.click();
    }, text);
  };

  // ① ② Android 语境
  {
    const page = await browser.newPage({ viewport: { width: 390, height: 844 }, hasTouch: true, userAgent: 'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 Chrome/120 Mobile Safari/537.36' });
    const errors = [];
    page.on('pageerror', (e) => errors.push(String(e).slice(0, 100)));
    await enterEditor(page);
    await page.evaluate(() => {
      window.__shareCalls = [];
      window.__openCalls = 0;
      navigator.canShare = () => true;
      navigator.share = async (d) => { window.__shareCalls.push({ n: d.files?.[0]?.name, type: d.files?.[0]?.type }); };
      window.open = () => { window.__openCalls++; return null; };
    });
    await openMenuClick(page, '导出 PDF');
    await page.waitForTimeout(900);
    const pdfState = await page.evaluate(() => ({
      shares: window.__shareCalls ?? [], openCalls: window.__openCalls,
      toast: document.querySelector('.toast')?.textContent ?? '',
      hasMenu: !!document.querySelector('.menu-pop'),
    }));
    record('① Android pdf→share 印式 html（撤 print）',
      pdfState.shares.length === 1 && /\.html$/.test(pdfState.shares[0].n ?? '') && pdfState.openCalls === 0,
      `share=${JSON.stringify(pdfState.shares)} window.open=${pdfState.openCalls} toast=${pdfState.toast}`);
    // ② pdf 后 App 活: 返回钮+再交互
    const backOk = await page.evaluate(() => {
      const btn = document.querySelector('.editor-bar button[aria-label="返回"]');
      return !!btn && btn.getBoundingClientRect().width > 0;
    });
    await page.click('.editor-bar button[aria-label="返回"]');
    await page.waitForTimeout(800);
    const appAlive = await page.evaluate(() => !!document.querySelector('.view') || !!document.querySelector('.note-card') || !!document.querySelector('.nt-row'));
    record('② pdf 后 App 活（返回可用）', backOk && appAlive, `返回钮=${backOk} 列表恢复=${appAlive}`);
    record('⑤a Android 零错误', errors.length === 0, errors.slice(0, 2).join('|') || 'clean');
    await page.close();
  }

  // ③ ④ word share 语义
  {
    const page = await browser.newPage({ viewport: { width: 390, height: 844 }, hasTouch: true });
    await enterEditor(page);
    // ③ share reject（组件拒绝）→ blob 兜底
    await page.evaluate(() => {
      navigator.canShare = () => true;
      navigator.share = async () => { const e = new Error('component refused'); e.name = 'NotAllowedError'; throw e; };
    });
    const dl = page.waitForEvent('download', { timeout: 8000 }).catch(() => null);
    await openMenuClick(page, '导出 Word');
    const download = await dl;
    await page.waitForTimeout(700);
    const t3 = await page.evaluate(() => document.querySelector('.toast')?.textContent ?? '');
    record('③ word share 拒绝→blob 真兜底（不假成功）', !!download && /已导出/.test(t3),
      `download=${download?.suggestedFilename() ?? 'null'} toast=${t3}`);
    // ④ AbortError（取消）→ ok 无 blob
    await page.evaluate(() => {
      navigator.share = async () => { const e = new Error('abort'); e.name = 'AbortError'; throw e; };
    });
    let dl4Fired = false;
    page.once('download', () => { dl4Fired = true; });
    await openMenuClick(page, '导出 TXT');
    await page.waitForTimeout(900);
    const t4 = await page.evaluate(() => document.querySelector('.toast')?.textContent ?? '');
    record('④ 取消=意图（无 blob 二次弹）', !dl4Fired && /已导出/.test(t4), `blob=${dl4Fired} toast=${t4}`);
    await page.close();
  }

  // ⑤ 桌面 pdf 保 print
  {
    const page = await browser.newPage({ viewport: { width: 390, height: 844 } });
    await enterEditor(page);
    await page.evaluate(() => { window.__openCalled = false; window.open = () => { window.__openCalled = true; return { document: { write: () => {}, close: () => {} }, focus: () => {}, print: () => {} }; }; });
    await openMenuClick(page, '导出 PDF');
    await page.waitForTimeout(900);
    const openCalled = await page.evaluate(() => window.__openCalled);
    record('⑤ 桌面 pdf 保 window.open+print 链', openCalled, `window.open=${openCalled}`);
    await page.close();
  }

  const passed = results.filter((r) => r.p).length;
  console.log(`\n=== E15 pdf/word 真机链: ${passed}/${results.length} PASS ===`);
  await browser.close();
  process.exit(passed === results.length ? 0 : 1);
})();
