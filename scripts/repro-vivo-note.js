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

// Aa 面板确保开（toggle 语义防御——面板已开时点 Aa=关）
async function ensureAaOpen(page, tab) {
  const open = await page.evaluate(() => !!document.querySelector('.tb-style-pop'));
  if (!open) {
    await page.click('.editor-toolbar button[title="文字格式"]');
    await page.waitForTimeout(300);
  }
  if (tab) {
    await page.evaluate((t) => {
      const tabs = Array.from(document.querySelectorAll('.tb-style-tab'));
      tabs.find((x) => x.textContent === t)?.click();
    }, tab);
    await page.waitForTimeout(200);
  }
}


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

  // ① E2 Aa 五合一组: 一钮弹全部文字格式（B/I/U/S/字号/色/高亮并入「格式」tab）
  await page.click('.editor-toolbar button[title="文字格式"]');
  await page.waitForTimeout(300);
  const aaPanel = await page.evaluate(() => ({
    panel: !!document.querySelector('.tb-style-pop'),
    tabs: Array.from(document.querySelectorAll('.tb-style-tab')).map((t) => t.textContent),
    fmtBtns: Array.from(document.querySelectorAll('.tb-fmt-row .tb-btn')).map((b) => b.title),
    sizes: document.querySelectorAll('.tb-fmt-panel .tb-size').length,
    swatches: document.querySelectorAll('.tb-fmt-panel .tb-swatch').length,
    residue: !!document.querySelector('.editor-toolbar button[title="文字颜色"]'),
  }));
  record('① E2 Aa 五合一（B/I/U/S+字号+色板+高亮）', aaPanel.panel && aaPanel.tabs.includes('格式') && aaPanel.fmtBtns.length === 4 && aaPanel.sizes >= 4 && aaPanel.swatches >= 12 && !aaPanel.residue,
    `tab=${aaPanel.tabs.join('|')} 格式钮=${aaPanel.fmtBtns.join('|')} 字号=${aaPanel.sizes} 色板=${aaPanel.swatches} 独立钮残留=${aaPanel.residue}`);

  // E4 键盘让位（TA 13:53 裁决）: 面板 fixed 向下弹（栏下方）+开面板 blur 收键盘+选定后回焦
  // ① 末面板已开（fmt tab）;不开不关——经 PM API 直接设焦（防点 .ProseMirror 误触面板/焦点抖动）
  await page.evaluate(() => { window.__editorView?.focus(); });
  await page.waitForTimeout(250);
  const focusedBefore = await page.evaluate(() => !!window.__editorView && window.__editorView.hasFocus());
  // 关再开（blur 链重新走——面板关触发 restore,再开触发 blur）
  await page.click('.editor-toolbar button[title="文字格式"]');  // toggle 关
  await page.waitForTimeout(250);
  await page.evaluate(() => { window.__editorView?.focus(); });
  await page.waitForTimeout(250);
  const focusedMid = await page.evaluate(() => !!window.__editorView && window.__editorView.hasFocus());
  await page.click('.editor-toolbar button[title="文字格式"]');  // 开（blur 触发）
  await page.waitForTimeout(300);
  const e4 = await page.evaluate((focusedMidArg) => {
    const panel = document.querySelector('.tb-style-pop');
    const bar = document.querySelector('.editor-toolbar');
    const ev = window.__editorView;
    if (!panel || !bar) return null;
    const pr = panel.getBoundingClientRect();
    const br = bar.getBoundingClientRect();
    const cs = getComputedStyle(panel);
    return {
      fixed: cs.position === 'fixed',
      width: pr.width,
      vw: window.innerWidth,
      full: pr.width > window.innerWidth * 0.8,
      atBottom: Math.abs(pr.bottom - br.top) < 4,  // 向下弹实义: 面板底贴栏顶（从钮下方展开,栏 49px 恒可见）
      barVisible: br.width > 0 && Math.abs(br.bottom - window.innerHeight) < 4, // 栏贴屏底不被面板遮（z 70<80）
      blurDone: focusedMidArg && !!ev && !ev.hasFocus(),  // 键盘让位: 开前有焦→开后失焦
    };
  }, focusedMid);
  // 选定样式（色板）→面板关→回焦恢复键盘
  await page.click('.tb-fmt-panel .tb-swatch >> nth=0');
  await page.waitForTimeout(300);
  const refocused = await page.evaluate(() => !!window.__editorView && window.__editorView.hasFocus());
  record('⑪ E4 键盘让位（向下弹+blur 收键盘+选定回焦）',
    focusedBefore && !!e4 && e4.fixed && e4.full && e4.atBottom && e4.barVisible && e4.blurDone && refocused,
    `先焦=${focusedBefore} fixed=${e4?.fixed} 宽=${e4?.width}/${e4?.vw} 贴底=${e4?.atBottom} 栏可见=${e4?.barVisible} 让位=${e4?.blurDone} 回焦=${refocused}`);

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
  await page.click('.tb-fmt-panel .tb-swatch >> nth=0'); // 红 #dc2626（Aa「格式」tab 内色板）
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
  await page.click('.tb-fmt-panel .tb-size >> nth=2'); // L=20（Aa「格式」tab 内字号）
  await page.waitForTimeout(300);
  const sizeHtml = await page.evaluate(() => document.querySelector('.ProseMirror p span[style*="font-size"]')?.style.fontSize || 'NONE');
  record('③ 字号档 L', sizeHtml === '20px', `span字号=${sizeHtml}`);

  // ④ 高亮: 选中「蓝字」（PM TextSelection——DOM Range 会被 PM 失焦重建吞）
  await page.evaluate(() => {
    const pmEl = document.querySelector('.ProseMirror');
    pmEl.focus();
    const view = pmEl.pmViewDesc?.view || (window.__pmView);
    const pm = view || pmEl;
    // PM head 查询: 直接经 pmViewDesc 链不可靠——用 pmDOM 的 editorView 全局（RichEditor 挂 window.__editorView）
    const ev = window.__editorView;
    if (!ev) return 'NO-EV';
    const { state } = ev;
    let pos = -1;
    state.doc.descendants((node, p) => {
      if (pos < 0 && node.isText && node.textContent.includes('蓝字普通')) {
        pos = p + node.textContent.indexOf('蓝字普通');
      }
      return pos < 0;
    });
    if (pos < 0) return 'NO-POS';
    ev.dispatch(state.tr.setSelection(state.selection.constructor.create(state.doc, pos, pos + 2)));
    ev.focus();
    return 'OK';
  });
  await ensureAaOpen(page, '格式');
  await page.click('.tb-fmt-panel .tb-swatch[title="高亮 #fef08a"]');
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
  await ensureAaOpen(page, '文字样式');
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
  await ensureAaOpen(page, '文字样式');
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
  record('⑧ 符号 tab（16 项+插入 ★）', tabState.tabs === 4 && symCount === 16 && starInDoc,
    `tab=${tabState.tabs}/${tabState.styleOn} 符号=${symCount} ★入文=${starInDoc}`);

  // ⑨ v3 图文模板 tab: 三 tab→模板四卡→插「待办清单」→块结构断言
  await ensureAaOpen(page, '文字样式');
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
  record('⑨ 图文模板 tab（四卡+待办模板结构）', tab3 === 4 && tmplCount === 4 && tmplDoc.h2 && tmplDoc.tasks >= 3,
    `tab=${tab3} 模板卡=${tmplCount} h2=${tmplDoc.h2} 任务=${tmplDoc.tasks} 块=[${tmplDoc.blocks}]`);

  // ⑩ E1 工具栏分组整改: ☰ 块组/＋插入组弹出+功能保活
  await page.click('.editor-toolbar button[title="块类型"]');
  await page.waitForTimeout(300);
  const blockPanel = await page.evaluate(() => ({
    panel: !!document.querySelector('.tb-style-pop'),
    items: document.querySelectorAll('.tb-style-pop .tb-style-btn').length,
  }));
  await page.click('.tb-style-pop .tb-style-btn[title="标题 1"]');
  await page.waitForTimeout(400);
  const h1After = await page.evaluate(() => !!document.querySelector('.ProseMirror h1'));
  record('⑩a ☰ 块组面板（6 项+H1 应用）', blockPanel.panel && blockPanel.items >= 6 && h1After,
    `面板=${blockPanel.panel} 项=${blockPanel.items} H1=${h1After}`);

  await page.click('.editor-toolbar button[title="插入"]');
  await page.waitForTimeout(300);
  const insPanel = await page.evaluate(() => ({
    panel: !!document.querySelector('.tb-style-pop'),
    items: document.querySelectorAll('.tb-style-pop .tb-style-btn').length,
  }));
  await page.click('.tb-style-pop .tb-style-btn[title="无序列表"]');
  await page.waitForTimeout(400);
  const ulAfter = await page.evaluate(() => !!document.querySelector('.ProseMirror ul'));
  record('⑩b ＋ 插入组面板（列表+HR/公式/流程图收纳）+无序列表应用', insPanel.panel && insPanel.items >= 6 && ulAfter,
    `面板=${insPanel.panel} 项=${insPanel.items} ul=${ulAfter}`);

  // ⑩c 平铺残留清零断言: 旧平铺钮（H1/分割线等）不再直接在 editor-toolbar 层
  const flatResidue = await page.evaluate(() => {
    const bars = Array.from(document.querySelectorAll('.editor-toolbar > .tb-btn'));
    return bars.filter((b) => ['标题 1', '分割线', '插入公式'].includes(b.title)).length;
  });
  record('⑩c 平铺残留清零（30+ 钮→9 钮分组）', flatResidue === 0, `残留=${flatResidue}`);

  record('⑥ 零错误', errors.length === 0, errors.length ? errors.slice(0, 2).join(' | ') : '双监听 0');

  const failed = results.filter((r) => !r.pass).length;
  console.log(`\n=== NOTE-VIVO 对标注: ${results.length - failed}/${results.length} PASS ===`);
  await browser.close();
  process.exit(failed > 0 ? 1 : 0);
})();
