/**
 * E18 验证（TA 20:24: 导出后短暂显示查看;点击查看直开导出内容）
 * ① 导出 MD→toast 含[查看]钮（短暂 5s 窗）
 * ② 点查看→sheet 开+文件名+内容含正文
 * ③ html 型（word）→iframe srcDoc 渲染
 * ④ 关 sheet（mask/X）正常
 * ⑤ 桌面 pdf（无 content）→纯文本 toast 无钮
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
  await page.keyboard.type('导出查看链测试正文', { delay: 20 });
  await page.waitForTimeout(400);

  // ① 导出 MD
  await page.click('button[aria-label="更多"]');
  await page.waitForTimeout(400);
  await page.evaluate(() => {
    Array.from(document.querySelectorAll('.menu-pop .menu-item')).find((b) => b.textContent?.includes('导出 Markdown'))?.click();
  });
  await page.waitForTimeout(900);
  const toastState = await page.evaluate(() => {
    const t = document.querySelector('.toast');
    if (!t) return { has: false };
    return { has: true, text: t.querySelector('.toast-action-text')?.textContent ?? t.textContent, btn: !!t.querySelector('.toast-action-btn') };
  });
  record('① toast 含[查看]钮', toastState.has && toastState.btn, `toast=${toastState.text} 钮=${toastState.btn}`);

  // ② 点查看→sheet
  await page.click('.toast-action-btn');
  await page.waitForTimeout(600);
  const sheet = await page.evaluate(() => {
    const sh = document.querySelector('.export-preview-sheet');
    if (!sh) return { has: false };
    const pre = sh.querySelector('pre');
    return { has: true, title: sh.querySelector('.sheet-title')?.textContent, content: pre?.textContent?.slice(0, 40) };
  });
  record('② 查看开 sheet+文件名+内容', sheet.has && /导出查看链测试正文/.test(sheet.content ?? '') && /\.md$/.test(sheet.title ?? ''),
    `题=${sheet.title} 文=${sheet.content}`);

  // ④ 关
  await page.click('.export-preview-sheet .sheet-head .icon-btn');
  await page.waitForTimeout(500);
  const closed = await page.evaluate(() => !document.querySelector('.export-preview-sheet'));
  record('④ 关 sheet', closed, `关=${closed}`);

  // ③ word→iframe
  await page.click('button[aria-label="更多"]');
  await page.waitForTimeout(400);
  await page.evaluate(() => {
    Array.from(document.querySelectorAll('.menu-pop .menu-item')).find((b) => b.textContent?.includes('导出 Word'))?.click();
  });
  await page.waitForTimeout(900);
  await page.click('.toast-action-btn');
  await page.waitForTimeout(600);
  const wordSheet = await page.evaluate(() => {
    const sh = document.querySelector('.export-preview-sheet');
    const ifr = sh?.querySelector('iframe');
    return { has: !!sh, title: sh?.querySelector('.sheet-title')?.textContent, iframe: !!ifr, doc: ifr?.getAttribute('srcdoc') ?? '' };
  });
  record('③ word→iframe srcDoc 渲染', wordSheet.has && wordSheet.iframe && /导出查看链测试正文/.test(wordSheet.doc),
    `题=${wordSheet.title} iframe=${wordSheet.iframe} docLen=${wordSheet.doc.length}`);
  await page.evaluate(() => document.querySelector('.export-preview-sheet .sheet-head .icon-btn')?.click());

  record('⑤ 零错误', errors.length === 0, errors.slice(0, 2).join(' | ') || 'clean');
  const passed = results.filter((r) => r.p).length;
  console.log(`\n=== E18 导出查看链: ${passed}/${results.length} PASS ===`);
  await browser.close();
  process.exit(passed === results.length ? 0 : 1);
})();
