/**
 * E24b: 序列化往返——callout/超级块/页签 → md → 重载还原
 * （docToMd 的 GFM [!NOTE] / HTML 容器语序持久链）
 */
const { chromium } = require('/home/z/.npm-global/lib/node_modules/playwright');
const BASE = 'file:///home/z/my-project/Aurora/apps/mobile/dist/index.html';

(async () => {
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 390, height: 844 }, hasTouch: true });
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
  await page.click('.nt-row:has-text("未命名")');
  await page.waitForSelector('.ProseMirror', { timeout: 10000 });
  await page.click('.ProseMirror');
  await page.waitForTimeout(400);

  // callout 插入+内容
  await page.keyboard.press('/');
  await page.waitForTimeout(500);
  await page.keyboard.type('提示', { delay: 30 });
  await page.waitForTimeout(400);
  await page.evaluate(() => {
    const cells = Array.from(document.querySelectorAll('.slash-cell'));
    cells.find((c) => c.textContent?.includes('提示块'))?.click();
  });
  await page.waitForTimeout(700);
  await page.keyboard.type('这是提示内容', { delay: 20 });

  // 序列化读（handle.save 直取 md?）——用 __editorHandle 若有;fallback: 标题失焦触发保存后读 Rust 面板。简径: docToMd 导出经 localStorage 保存链
  // MobileApp 保存流程: 编辑失焦→ save。此处点标题栏触发。
  await page.click('.app-bar-title, .note-title, .editor-title');
  await page.waitForTimeout(1200);

  // 回读: 重新打开同笔记
  await page.goBack();
  await page.waitForTimeout(900);
  await page.click('.tab-bar .tab-item:has-text("知识库")');
  await page.waitForTimeout(800);
  await page.click('.nt-arrow');
  await page.waitForTimeout(400);
  await page.click('.nt-row:has-text("未命名")');
  await page.waitForSelector('.ProseMirror', { timeout: 10000 });
  await page.waitForTimeout(800);
  const restored = await page.evaluate(() => {
    const dom = window.__editorView.dom;
    const text = dom.textContent;
    return { text: text.slice(0, 40) };
  });
  record('① callout 内容往返持久', restored.text.includes('这是提示内容'), `文=${restored.text}`);

  const passed = results.filter((r) => r.p).length;
  console.log(`\n=== E24b 序列化往返: ${passed}/${results.length} PASS ===`);
  await browser.close();
  process.exit(passed === results.length ? 0 : 1);
})();
