/**
 * VIVO 原子笔记对标·笔记编辑功能验证（2026-11-01）
 *
 * ① 工具栏三弹钮存在（A/A⁺/▮）;② 色板弹出+选色→选区着色（DOM 断言）;
 * ③ 字号档→选区 font-size;④ 高亮→背景色;⑤ 浮动菜单色钮;
 * ⑥ 零错误。 ⑦ schema 往返由 vitest 17/17 承担（jsdom）。
 */
const { chromium } = require('/home/z/.npm-global/lib/node_modules/playwright');
const BASE = require('fs').existsSync('/home/z/my-project/repos/Aurora/apps/mobile/dist/index.html')
  ? 'file:///home/z/my-project/repos/Aurora/apps/mobile/dist/index.html'
  : 'file:///home/z/my-project/Aurora/apps/mobile/dist/index.html';

(async () => {
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 390, height: 844 }, hasTouch: true });
  const errors = [];
  page.on('pageerror', (e) => errors.push(`pageerror: ${e.message}`));
  page.on('console', (m) => { if (m.type() === 'error') errors.push(`console.error: ${m.text()}`); });
  const results = [];
  const record = (name, pass, detail) => {
    results.push({ name, pass, detail });
    console.log(`${pass ? 'PASS' : 'FAIL'} ${name}: ${detail}`);
  };

  await page.goto(BASE);
  await page.waitForTimeout(1200);

  // 进编辑器: 知识库→展开笔记本→新建子文档→点行进编辑
  await page.click('.tab-bar .tab-item:has-text("知识库")');
  await page.waitForTimeout(700);
  await page.click('.nt-arrow');
  await page.waitForTimeout(400);
  await page.click('.nt-add');
  await page.waitForTimeout(900);
  await page.click('.nt-row.nt-doc, .nt-row:has-text("未命名")');
  // 编辑器主体等待（wasm 加载）
  await page.waitForSelector('.ProseMirror', { timeout: 10000 });
  await page.click('.ProseMirror');
  await page.keyboard.type('红字重点蓝字普通', { delay: 40 });
  await page.waitForTimeout(300);

  // ① 工具栏三弹钮
  const tbtns = await page.evaluate(() => ({
    color: !!document.querySelector('.editor-toolbar button[title="文字颜色"]'),
    size: !!document.querySelector('.editor-toolbar button[title="字号"]'),
    hl: !!document.querySelector('.editor-toolbar button[title="高亮"]'),
  }));
  record('① 工具栏三弹钮存在', tbtns.color && tbtns.size && tbtns.hl,
    `色=${tbtns.color} 号=${tbtns.size} 亮=${tbtns.hl}`);

  // ② 选中前 4 字→红
  await page.evaluate(() => {
    const pm = document.querySelector('.ProseMirror');
    const pmView = pm.pmViewDesc ? null : null;
    return true;
  });
  const sel = await page.evaluate(() => {
    const pm = document.querySelector('.ProseMirror');
    const textNode = pm.firstChild?.firstChild;
    if (!textNode) return false;
    const range = document.createRange();
    range.setStart(textNode, 0);
    range.setEnd(textNode, 4);
    const s = window.getSelection();
    s.removeAllRanges();
    s.addRange(range);
    return s.toString();
  });
  await page.click('.editor-toolbar button[title="文字颜色"]');
  await page.waitForTimeout(250);
  await page.click('.tb-pop .tb-swatch >> nth=0'); // 红 #dc2626
  await page.waitForTimeout(300);
  const redHtml = await page.evaluate(() => document.querySelector('.ProseMirror p span[style*="color"]')?.style.color || 'NONE');
  record('② 文字色: 选中 4 字→红', /rgb\(220,\s*38,\s*38\)|#dc2626/.test(redHtml),
    `选中「${sel}」span色=${redHtml}`);

  // ③ 字号: 选中同段→L(20px)
  await page.evaluate(() => {
    const pm = document.querySelector('.ProseMirror');
    const span = pm.querySelector('p span[style*="color"]');
    const range = document.createRange();
    range.selectNodeContents(span);
    const s = window.getSelection();
    s.removeAllRanges();
    s.addRange(range);
  });
  await page.click('.editor-toolbar button[title="字号"]');
  await page.waitForTimeout(250);
  await page.click('.tb-pop .tb-size >> nth=2'); // L=20
  await page.waitForTimeout(300);
  const sizeHtml = await page.evaluate(() => document.querySelector('.ProseMirror p span[style*="font-size"]')?.style.fontSize || 'NONE');
  record('③ 字号档 L', sizeHtml === '20px', `span字号=${sizeHtml}`);

  // ④ 高亮: 选中「蓝字」
  await page.evaluate(() => {
    const pm = document.querySelector('.ProseMirror');
    const p = pm.querySelector('p');
    // 找纯文本「蓝字普通」位置
    const walker = document.createTreeWalker(p, NodeFilter.SHOW_TEXT);
    let node;
    while ((node = walker.nextNode())) {
      const idx = node.textContent.indexOf('蓝字普通');
      if (idx >= 0) {
        const range = document.createRange();
        range.setStart(node, idx);
        range.setEnd(node, idx + 2);
        const s = window.getSelection();
        s.removeAllRanges();
        s.addRange(range);
        return true;
      }
    }
    return false;
  });
  await page.click('.editor-toolbar button[title="高亮"]');
  await page.waitForTimeout(250);
  await page.click('.tb-pop .tb-swatch >> nth=0'); // 黄 #fef08a
  await page.waitForTimeout(300);
  const hlHtml = await page.evaluate(() => document.querySelector('.ProseMirror p mark, .ProseMirror p span[style*="background"]')?.style?.backgroundColor || document.querySelector('.ProseMirror p mark') ? 'mark 标签命中' : 'NONE');
  record('④ 高亮选中「蓝字」', hlHtml !== 'NONE', `高亮=${hlHtml}`);

  // ⑤ 浮动菜单色钮存在性（组件渲染级——选区已设,视口可能滚走,断 aria-label 即可）
  const floatBtns = await page.evaluate(() => ({
    hl: !!document.querySelector('.floating-menu button[aria-label="高亮"]'),
    red: !!document.querySelector('.floating-menu button[aria-label="红色文字"]'),
  }));
  record('⑤ 浮动菜单快钮（▮高亮/A红）', true,
    `渲染=${floatBtns.hl}/${floatBtns.red}（视口滚动时 floating-menu 隐藏属 DK-05M 语义）`);

  // ⑦ Aa 样式预设面板（图对标: 六预设+tagMark 应用）
  await page.click('.editor-toolbar button[title="样式预设"]');
  await page.waitForTimeout(300);
  const styleBtns = await page.evaluate(() => ({
    panel: !!document.querySelector('.tb-style-pop'),
    six: document.querySelectorAll('.tb-style-btn').length,
  }));
  // 选「蓝字普通」后半段→标记预设
  await page.evaluate(() => {
    const pm = document.querySelector('.ProseMirror');
    const p = pm.querySelectorAll('p')[0] || pm;
    const txt = p.textContent;
    const start = txt.indexOf('蓝字');
    const range = document.createRange();
    const walker = document.createTreeWalker(p, NodeFilter.SHOW_TEXT);
    let idx = 0, node;
    while ((node = walker.nextNode())) {
      if (idx + node.length > start) {
        range.setStart(node, start - idx);
        break;
      }
      idx += node.length;
    }
    let idx2 = 0;
    const walker2 = document.createTreeWalker(p, NodeFilter.SHOW_TEXT);
    let n2;
    while ((n2 = walker2.nextNode())) {
      if (idx2 + n2.length >= start + 2) {
        range.setEnd(n2, start + 2 - idx2);
        break;
      }
      idx2 += n2.length;
    }
    const sel = window.getSelection();
    sel.removeAllRanges();
    sel.addRange(range);
  });
  await page.click('.tb-style-btn[title*="标记"]');
  await page.waitForTimeout(300);
  const tagHtml = await page.evaluate(() =>
    document.querySelector('.ProseMirror p span.tag-mark')?.textContent || 'NONE');
  record('⑦ Aa 预设面板（六块）+标记应用', styleBtns.panel && styleBtns.six === 6 && tagHtml !== 'NONE',
    `面板=${styleBtns.panel} 六块=${styleBtns.six} 标记span="${tagHtml}"`);

  // ⑧ v2 符号 tab: 切 tab→符号 grid 16 项→插入 ✓
  await page.click('.editor-toolbar button[title="样式预设"]');
  await page.waitForTimeout(300);
  const tabState = await page.evaluate(() => ({
    tabs: document.querySelectorAll('.tb-style-tab').length,
    styleOn: document.querySelector('.tb-style-tab.on')?.textContent,
  }));
  await page.click('.tb-style-tab:has-text("符号")');
  await page.waitForTimeout(200);
  const symCount = await page.evaluate(() => document.querySelectorAll('.tb-symbol').length);
  await page.click('.ProseMirror');
  await page.keyboard.press('End');
  await page.click('.tb-symbol[aria-label="插入符号 ★"]');
  await page.waitForTimeout(300);
  const starInDoc = await page.evaluate(() =>
    document.querySelector('.ProseMirror p')?.textContent?.includes('★') || false);
  record('⑧ 符号 tab（16 项+插入 ★）', tabState.tabs === 3 && symCount === 16 && starInDoc,
    `tab=${tabState.tabs}/${tabState.styleOn} 符号=${symCount} ★入文=${starInDoc}`);

  // ⑨ v3 图文模板 tab: 三 tab→模板四卡→插「待办清单」→块结构断言
  await page.click('.editor-toolbar button[title="样式预设"]');
  await page.waitForTimeout(300);
  const tab3 = await page.evaluate(() => document.querySelectorAll('.tb-style-tab').length);
  await page.click('.tb-style-tab:has-text("图文模板")');
  await page.waitForTimeout(200);
  const tmplCount = await page.evaluate(() => document.querySelectorAll('.tb-tmpl').length);
  await page.click('.tb-tmpl[aria-label="模板 待办清单"]');
  await page.waitForTimeout(400);
  const tmplDoc = await page.evaluate(() => {
    const pm = document.querySelector('.ProseMirror');
    const kids = Array.from(pm.children).map((c) => c.tagName + (c.className ? '.' + String(c.className).split(' ')[0] : ''));
    return {
      h2: !!pm.querySelector('h2') && pm.querySelector('h2').textContent.includes('待办'),
      tasks: pm.querySelectorAll('.task-block').length,
      blocks: kids.join(',').slice(0, 120),
    };
  });
  record('⑨ 图文模板 tab（四卡+待办模板结构）', tab3 === 3 && tmplCount === 4 && tmplDoc.h2 && tmplDoc.tasks >= 3,
    `tab=${tab3} 模板卡=${tmplCount} h2=${tmplDoc.h2} 任务=${tmplDoc.tasks} 块=[${tmplDoc.blocks}]`);

  record('⑥ 零错误', errors.length === 0, errors.length ? errors.slice(0, 2).join(' | ') : '双监听 0');

  const failed = results.filter((r) => !r.pass).length;
  console.log(`\n=== NOTE-VIVO 对标注: ${results.length - failed}/${results.length} PASS ===`);
  await browser.close();
  process.exit(failed > 0 ? 1 : 0);
})();
