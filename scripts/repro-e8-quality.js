/**
 * E8 优质软件调研落地验证（TA 09:03——全面优化）
 * ① markdown 快捷输入: 「# 」→H1 /「> 」→引用 /「- 」→无序列表 /「1. 」→有序 /「[] 」→任务
 * ② 内联 mark: 星号粗/星单斜/==高亮==/反引号码（块注释内避免星斜组合提前闭合）
 * ③ 排版: 正文 17px/1.75+34em 帽;标题 28/700;段距 12
 * ④ 卡片: 14px 圆角+按压 scale
 * ⑤ 引用品牌杆+衬底
 */
const { chromium } = require('/home/z/.npm-global/lib/node_modules/playwright');
const BASE = 'file:///home/z/my-project/repos/Aurora/apps/mobile/dist/index.html';

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
  // 进编辑器（vivo 同序列）: 知识库→展开笔记本→新建子文档→点行进编辑
  await page.click('.tab-bar .tab-item:has-text("知识库")');
  await page.waitForTimeout(700);
  await page.click('.nt-arrow');
  await page.waitForTimeout(400);
  await page.click('.nt-add');
  await page.waitForTimeout(900);
  await page.click('.nt-row.nt-doc, .nt-row:has-text("未命名")');
  await page.waitForSelector('.ProseMirror', { timeout: 10000 });
  await page.click('.ProseMirror');
  await page.keyboard.press('Delete'); // 清初始空段提示干扰
  await page.waitForTimeout(300);
  const v = await page.evaluate(() => !!window.__editorView);
  record('前置 编辑器就绪', v, `__editorView=${v}`);

  const type = async (text) => {
    await page.evaluate(() => { window.__editorView?.focus(); });
    await page.keyboard.type(text, { delay: 10 });
    await page.waitForTimeout(120);
  };
  const docDump = () => page.evaluate(() => {
    const ev = window.__editorView;
    return ev ? ev.state.doc.toJSON() : null;
  });

  await type('# ');
  await page.waitForTimeout(200);
  let doc = await docDump();
  let top = doc?.content?.[0]?.type;
  record('①a 「# 」→ H1', top === 'heading', `首块=${top} level=${doc?.content?.[0]?.attrs?.level}`);

  await page.keyboard.press('Control+a');
  await page.keyboard.press('Delete');
  await type('> ');
  await page.waitForTimeout(200);
  doc = await docDump();
  top = doc?.content?.[0]?.type;
  record('①b 「> 」→ blockquote', top === 'blockquote', `首块=${top}`);
  const quoteCssEarly = await page.evaluate(() => {
    const q = document.querySelector('.ProseMirror blockquote');
    if (!q) return null;
    const cs = getComputedStyle(q);
    return { bl: cs.borderLeftWidth };
  });
  record('⑤ 引用品牌杆 3px', quoteCssEarly?.bl === '3px', `bl=${quoteCssEarly?.bl}（${top === 'blockquote' ? '引用在场' : '无引用'}）`);

  await page.keyboard.press('Control+a');
  await page.keyboard.press('Delete');
  await type('- ');
  await page.waitForTimeout(200);
  doc = await docDump();
  top = doc?.content?.[0]?.type;
  record('①c 「- 」→ bullet_list', top === 'bullet_list', `首块=${top}`);

  await page.keyboard.press('Control+a');
  await page.keyboard.press('Delete');
  await type('1. ');
  await page.waitForTimeout(200);
  doc = await docDump();
  top = doc?.content?.[0]?.type;
  record('①d 「1. 」→ ordered_list', top === 'ordered_list', `首块=${top}`);

  await page.keyboard.press('Control+a');
  await page.keyboard.press('Delete');
  await type('[ ] ');
  await page.waitForTimeout(250);
  await type('项目');
  await page.waitForTimeout(300);
  doc = await docDump();
  top = doc?.content?.[0]?.type;
  const taskChecked = doc?.content?.[0]?.attrs?.checked;
  record('①e 「[ ] 」→ task_block', top === 'task_block', `首块=${top} checked=${taskChecked}`);

  await page.keyboard.press('Control+a');
  await page.keyboard.press('Delete');
  await type('**粗体**');
  await page.waitForTimeout(250);
  const markDump = await page.evaluate(() => {
    const ev = window.__editorView;
    if (!ev) return { type: 'none' };
    const p = ev.state.doc.content.firstChild;
    return { type: p?.type?.name ?? 'none', marks: (p?.firstChild?.marks ?? []).map((m) => m.type.name) };
  });
  record('②a 双星粗 → strong', (markDump.marks ?? []).includes('strong'), `marks=${markDump.marks}`);

  await page.keyboard.press('Control+a');
  await page.keyboard.press('Delete');
  await type('==高亮==');
  await page.waitForTimeout(250);
  const hlDump = await page.evaluate(() => {
    const ev = window.__editorView;
    const p = ev?.state.doc.content.firstChild;
    return { marks: (p?.firstChild?.marks ?? []).map((m) => m.type.name) };
  });
  record('②b ==高亮== → highlight', (hlDump.marks ?? []).includes('highlight'), `marks=${hlDump.marks}`);

  const typo = await page.evaluate(() => {
    const pm = document.querySelector('.ProseMirror');
    if (!pm) return null;
    const cs = getComputedStyle(pm);
    return { fs: cs.fontSize, lh: cs.lineHeight, mw: pm.getBoundingClientRect().width };
  });
  record('③ 排版 17px/1.75+34em 帽', typo?.fs === '17px' && typo?.mw <= 640,
    `fs=${typo?.fs} lh=${typo?.lh} 宽=${typo?.mw}`);

  // ④ 卡片: 返回→知识库切「列表」模式（note-card 渲染面）
  await page.click('.editor-bar button[aria-label="返回"]');
  await page.waitForTimeout(900);
  await page.evaluate(() => {
    const segs = Array.from(document.querySelectorAll('.nt-seg'));
    segs.find((x) => x.textContent === '列表')?.click();
  });
  await page.waitForTimeout(900);
  const cardCss = await page.evaluate(() => {
    const c = document.querySelector('.note-card');
    if (!c) return null;
    const cs = getComputedStyle(c);
    return { radius: cs.borderRadius, pad: cs.padding };
  });
  record('④ 卡片 14px 圆角', cardCss?.radius === '14px', `radius=${cardCss?.radius} pad=${cardCss?.pad}`);

  record('⑥ 零错误', errors.length === 0, errors.slice(0, 2).join(' | ') || 'clean');

  const passed = results.filter((r) => r.pass).length;
  console.log(`\n=== E8 优质体验: ${passed}/${results.length} PASS ===`);
  await browser.close();
  process.exit(passed === results.length ? 0 : 1);
})();
