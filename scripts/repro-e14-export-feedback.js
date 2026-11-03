/**
 * E14 验证（TA 18:16: ⋮→导入导出无反应→修反馈链）
 * ① 导出 Markdown→toast「已导出 x.md」+download 走（headless blob 通路）
 * ② 导出 Word/TXT 同反馈
 * ③ canShare 环境模拟: share 双通道分派（navigator.share mock 收到 File）
 * ④ 失败链: 模块异常→toast 错（不静默）
 * ⑤ 菜单点击链稳定（⋮→项→menu 关）
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
  const enterEditor = async () => {
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
    await page.keyboard.type('导出测试内容', { delay: 20 });
    await page.waitForTimeout(300);
  };
  const openMenu = async () => {
    await page.click('button[aria-label="更多"]');
    await page.waitForTimeout(400);
  };
  const toastText = () => page.evaluate(() => document.querySelector('.toast')?.textContent ?? '');

  // ① ② ③ blob 通路（headless 无 share）
  await enterEditor();
  await openMenu();
  const dl = page.waitForEvent('download', { timeout: 8000 }).catch(() => null);
  await page.evaluate(() => {
    const btn = Array.from(document.querySelectorAll('.menu-pop .menu-item')).find((b) => b.textContent?.includes('导出 Markdown'));
    btn?.click();
  });
  const download = await dl;
  await page.waitForTimeout(600);
  const t1 = await toastText();
  record('① 导出 MD: toast 反馈+下载产物', !!t1 && !!download, `toast=${t1} download=${download?.suggestedFilename() ?? 'null'}`);

  // ② Word
  await openMenu();
  const dl2 = page.waitForEvent('download', { timeout: 8000 }).catch(() => null);
  await page.evaluate(() => {
    const btn = Array.from(document.querySelectorAll('.menu-pop .menu-item')).find((b) => b.textContent?.includes('导出 Word'));
    btn?.click();
  });
  const download2 = await dl2;
  await page.waitForTimeout(600);
  const t2 = await toastText();
  record('② 导出 Word: toast+产物', !!t2 && !!download2, `toast=${t2} download=${download2?.suggestedFilename() ?? 'null'}`);

  // ③ share 环境模拟（真机链）: mock canShare/share→点导出→share 收 File
  await page.evaluate(() => {
    window.__shareCalls = [];
    navigator.canShare = (d) => !!(d?.files?.length);
    navigator.share = async (d) => { window.__shareCalls.push({ n: d.files?.[0]?.name, s: d.files?.[0]?.size }); };
  });
  await openMenu();
  await page.evaluate(() => {
    const btn = Array.from(document.querySelectorAll('.menu-pop .menu-item')).find((b) => b.textContent?.includes('导出 TXT'));
    btn?.click();
  });
  await page.waitForTimeout(600);
  const shareState = await page.evaluate(() => ({ calls: window.__shareCalls ?? [], toast: document.querySelector('.toast')?.textContent ?? '' }));
  record('③ share 双通道（真机链: 系统面板接管）', shareState.calls.length === 1 && !!shareState.toast,
    `share=${JSON.stringify(shareState.calls)} toast=${shareState.toast}`);
  await page.evaluate(() => { delete navigator.canShare; delete navigator.share; });

  // ④ 失败链: 编辑器未就绪（view 清）→导出→toast 错（不静默）
  await page.evaluate(() => { window.__editorView = undefined; });
  await openMenu();
  await page.evaluate(() => {
    const btn = Array.from(document.querySelectorAll('.menu-pop .menu-item')).find((b) => b.textContent?.includes('导出 Markdown'));
    btn?.click();
  });
  await page.waitForTimeout(900);
  const t4 = await toastText();
  record('④ 失败链 toast（不静默）', /导出失败/.test(t4), `toast=${t4}`);

  record('⑤ 零错误', errors.length === 0, errors.slice(0, 2).join(' | ') || 'clean');
  const passed = results.filter((r) => r.p).length;
  console.log(`\n=== E14 导出反馈链: ${passed}/${results.length} PASS ===`);
  await browser.close();
  process.exit(passed === results.length ? 0 : 1);
})();
