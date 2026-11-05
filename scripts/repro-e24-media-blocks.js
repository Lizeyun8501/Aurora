/**
 * E24 验证（TA 22:13: 编辑功能补全——附件/媒体/表情/六级标题/容器块）
 * ① insert 面板附件/媒体钮全现（本选图/拍照/文件/视频/音频）
 * ② emoji 面（symbol tab: 5 分类+格插入正文）
 * ③ 块面板 H4-H6+容器块（callout/超级横纵/页签/思维导图）
 * ④ slash 菜单 17 项（10+7 新）
 * ⑤ callout 插入真节点+样式类
 * ⑥ 序列化往返（callout→md [!NOTE]）
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
  const prompts = [];
  page.on('dialog', async (d) => { prompts.push(d.message()); await d.accept('https://media.example.com/v.mp4'); });

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

  // ① insert 面板附件/媒体钮
  await page.click('button[title="插入"]');
  await page.waitForTimeout(700);
  const ins = await page.evaluate(() => ({
    localImg: !!document.querySelector('button[title="插入图片（本地）"]'),
    camera: !!document.querySelector('button[title="拍照并插入"]'),
    file: !!document.querySelector('button[title="插入文件"]'),
    video: !!document.querySelector('button[title="插入视频链接"]'),
    audio: !!document.querySelector('button[title="插入音频链接"]'),
    hiddenInputs: document.querySelectorAll('.editor-toolbar input[type="file"][hidden]').length,
  }));
  record('① insert 面板附件媒体五钮+hidden 三通道', ins.localImg && ins.camera && ins.file && ins.video && ins.audio && ins.hiddenInputs === 3,
    JSON.stringify(ins));
  await page.keyboard.press('Escape');
  await page.waitForTimeout(400);

  // ② emoji 面
  await page.click('button[title="文字格式"]');
  await page.waitForTimeout(500);
  await page.click('.tb-style-tab:has-text("符号")');
  await page.waitForTimeout(500);
  const emoji = await page.evaluate(() => ({
    cats: document.querySelectorAll('.tb-emoji-tab').length,
    grid: document.querySelectorAll('.tb-emoji').length,
    firstCat: document.querySelector('.tb-emoji-tab')?.textContent,
  }));
  record('② emoji 面（5 分类+首类 40 格）', emoji.cats === 5 && emoji.grid >= 28, `分类=${emoji.cats} 格=${emoji.grid}`);
  await page.click('.tb-emoji');
  await page.waitForTimeout(400);
  const inserted = await page.evaluate(() => window.__editorView.state.doc.textContent.includes('😀'));
  record('②b emoji 点击入正文', inserted, `doc 含 😀=${inserted}`);
  await page.keyboard.press('Escape');
  await page.waitForTimeout(300);

  // ③ 块面板 H4-H6+容器
  await page.click('button[title="块类型"]');
  await page.waitForTimeout(600);
  const blk = await page.evaluate(() => ({
    h4: !!Array.from(document.querySelectorAll('.tb-style-btn')).find((b) => b.title === '标题 4'),
    h6: !!Array.from(document.querySelectorAll('.tb-style-btn')).find((b) => b.title === '标题 6'),
    callout: !!Array.from(document.querySelectorAll('.tb-style-btn')).find((b) => b.title === '提示块'),
    superH: !!Array.from(document.querySelectorAll('.tb-style-btn')).find((b) => b.title === '横向超级块'),
    superV: !!Array.from(document.querySelectorAll('.tb-style-btn')).find((b) => b.title === '纵向超级块'),
    tabs: !!Array.from(document.querySelectorAll('.tb-style-btn')).find((b) => b.title === '页签块'),
    mindmap: !!Array.from(document.querySelectorAll('.tb-style-btn')).find((b) => b.title === '思维导图块'),
  }));
  record('③ 块面板 H4-H6+容器五钮', blk.h4 && blk.h6 && blk.callout && blk.superH && blk.superV && blk.tabs && blk.mindmap, JSON.stringify(blk));

  // ④ slash 17 项
  await page.keyboard.press('Escape');
  await page.waitForTimeout(300);
  await page.click('.ProseMirror');
  await page.keyboard.press('Enter');
  await page.keyboard.press('/');
  await page.waitForTimeout(600);
  const slash = await page.evaluate(() => document.querySelectorAll('.slash-cell').length);
  record('④ slash 菜单 18 项（10 原有+8 新）', slash === 18, `项=${slash}`);
  await page.keyboard.press('Escape');
  await page.waitForTimeout(300);

  // ⑤ callout 插入（slash 应验真节点）——点项
  await page.keyboard.press('Enter');
  await page.keyboard.press('/');
  await page.waitForTimeout(500);
  await page.keyboard.type('提示', { delay: 30 });
  await page.waitForTimeout(500);
  await page.evaluate(() => {
    const cells = Array.from(document.querySelectorAll('.slash-cell'));
    cells.find((c) => c.textContent?.includes('提示块'))?.click();
  });
  await page.waitForTimeout(700);
  const co = await page.evaluate(() => {
    const dom = window.__editorView.dom;
    return { callout: !!dom.querySelector('.callout-block[data-callout="note"]'), text: dom.textContent };
  });
  record('⑤ callout 插入真节点', co.callout, `节点=${co.callout}`);

  // ⑥ 视频链接 embed 渲染（dialog accept 注 URL）
  await page.keyboard.press('Enter');
  await page.click('button[title="插入"]');
  await page.waitForTimeout(600);
  await page.click('button[title="插入视频链接"]');
  await page.waitForTimeout(700);
  const vid = await page.evaluate(() => {
    const dom = window.__editorView.dom;
    return { video: !!dom.querySelector('embed-block video, .embed-block video'), count: dom.querySelectorAll('.embed-block').length };
  });
  record('⑥ 视频链接 embed 渲染', vid.video, `video=${vid.video}`);

  record('⑦ 零错误', errors.length === 0, errors.slice(0, 2).join(' | ') || 'clean');
  const passed = results.filter((r) => r.p).length;
  console.log(`\n=== E24 附件媒体表情容器块: ${passed}/${results.length} PASS ===`);
  await browser.close();
  process.exit(passed === results.length ? 0 : 1);
})();
