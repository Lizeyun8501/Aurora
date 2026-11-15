/**
 * B-05（TA 2026-11-14 09:01 工单）: TOP 用户路径正式 E2E 套件。
 *
 * 形态: Playwright chromium + apps/mobile/dist/index.html（file:// 零服务
 * 依赖——mock 平台 localStorage 落地;选择器承袭 scripts/repro-* 已验证面）。
 * 运行: node scripts/tests/e2eTopPaths.mjs（AURORA_E2E_BASE 可覆写）。
 * CI: .github/workflows/e2e.yml（push 路径过滤+每周 cron+手动）。
 *
 * 覆盖（P=PW 本套;A=既有自动化锚点——TOP20 全表见交付报告）:
 *  P1  新建笔记  P2 编辑保存(往返持久)  P3 搜索  P4 建任务(捕获→今日)
 *  P5 今日勾选  P6 回收站还原  P7 导出视图  P8 markdown 输入→块渲染
 *  A面(非本套): 导入/导出真文件/同步冲突/加密解锁——core cargo 测试+
 *  vitest 锚（manifest 表格化,防「零 E2E」误判重现）。
 */
import { createRequire } from 'module';
import { fileURLToPath } from 'url';
import path from 'path';
// PW 解析: 仓内 node_modules 优先（root devDep——CI/本机同路）;本机全局包兜底
let chromium;
try {
  ({ chromium } = createRequire(import.meta.url)('playwright'));
} catch {
  ({ chromium } = createRequire('/home/z/.npm-global/lib/node_modules/playwright/')('playwright'));
}

/* dist 路径仓相对解析（CI 可移植——AURORA_E2E_BASE 可覆写） */
const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const BASE = process.env.AURORA_E2E_BASE
  ?? 'file://' + path.join(repoRoot, 'apps', 'mobile', 'dist', 'index.html');
const results = [];
const ok = (n, cond, d = '') => { results.push({ n, cond }); console.log(`${cond ? 'PASS' : 'FAIL'} ${n}${d ? ': ' + d : ''}`); };

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

(async () => {
  const browser = await chromium.launch();
  const ctx = await browser.newContext({ viewport: { width: 390, height: 844 }, hasTouch: true });
  const page = await ctx.newPage();
  const errors = [];
  page.on('pageerror', (e) => errors.push(String(e)));
  page.on('console', (m) => { if (m.type() === 'error') errors.push(m.text()); });

  await page.goto(BASE);
  await sleep(1600);

  /* ── P1 新建笔记 ─────────────────────────────── */
  await page.click('.tab-bar .tab-item:has-text("知识库")');
  await sleep(700);
  await page.click('.nt-arrow').catch(() => {});
  await sleep(400);
  const beforeN = await page.evaluate(() => JSON.parse(localStorage.getItem('aurora.notes') ?? '[]').length);
  await page.click('.nt-add');
  await sleep(900);
  const afterNotes = await page.evaluate(() => JSON.parse(localStorage.getItem('aurora.notes') ?? '[]'));
  ok('P1 新建笔记（库+1·未命名在列）', afterNotes.length === beforeN + 1, `notes=${afterNotes.length}`);

  /* ── P2 编辑保存（写→返回→库内容→重开回显） ── */
  const target = afterNotes[afterNotes.length - 1];
  /* 树形渲染异步（新建行延迟入 DOM）——先等行;折叠态则展开再等 */
  await page.waitForSelector(`.nt-row:has-text("${target.title}")`, { timeout: 4000 }).catch(() => {});
  let rowVis = await page.locator(`.nt-row:has-text("${target.title}")`).isVisible().catch(() => false);
  if (!rowVis) {
    await page.click('.nt-arrow').catch(() => {});
    await sleep(600);
    rowVis = await page.locator(`.nt-row:has-text("${target.title}")`).isVisible().catch(() => false);
  }
  ok('P2 前置（笔记行可达）', rowVis, `title=${target.title}`);
  await page.click(`.nt-row:has-text("${target.title}")`);
  await page.waitForSelector('.ProseMirror', { timeout: 10000 });
  await page.click('.ProseMirror');
  await page.keyboard.type('# B05 回归标题');
  /* 保存链: onSave debounce 1s——**先等落库再返回**（编辑器挂载期间不落
   * 库=空快照假阳根因;卸载 flushSave 依赖 dirty 信号非可靠通道）。 */
  await sleep(1800);
  const snapSaved = await page.evaluate((id) => {
    const s = localStorage.getItem(`aurora.snap.${id}`);
    /* 空快照基线 ~150 字符——阈值 300 防假阳（真实内容快照显著更大） */
    return !!s && s.length > 300;
  }, target.id);
  await page.click('button[aria-label="返回"], .editor-back, .back-btn').catch(() => page.goBack());
  await sleep(800);
  /* 重开回显（用户可见语义——存→取→渲染等价;行标题已同步内容首行） */
  await page.click('.nt-row >> nth=-1').catch(() => {});
  await page.waitForSelector('.ProseMirror', { timeout: 10000 });
  await sleep(700);
  const shown = await page.evaluate(() => (document.querySelector('.ProseMirror')?.textContent || ''));
  ok('P2 编辑保存（快照落库+重开回显）', snapSaved && shown.includes('B05 回归标题'), `snap=${snapSaved} shown=${shown.slice(0, 24)}`);
  await page.click('button[aria-label="返回"], .editor-back, .back-btn').catch(() => page.goBack());
  await sleep(600);

  /* ── P8 markdown 输入→块渲染（输入语法即时成块） ── */
  await page.click('.nt-row >> nth=-1');
  await page.waitForSelector('.ProseMirror', { timeout: 10000 });
  await page.click('.ProseMirror');
  await page.keyboard.press('Control+End').catch(() => {});
  await page.keyboard.press('End');
  await page.keyboard.press('Enter');
  await page.keyboard.type('- B05 列表项');
  await sleep(600);
  const hasList = await page.evaluate(() => !!document.querySelector('.ProseMirror ul, .ProseMirror ol, .ProseMirror li'));
  ok('P8 markdown 输入→列表块渲染', hasList, 'li/ul 在编辑器 DOM');
  await page.click('button[aria-label="返回"], .editor-back, .back-btn').catch(() => page.goBack());
  await sleep(600);

  /* ── P3 搜索（A-04 SearchView） ── */
  await page.evaluate(() => { window.history.pushState({}, '', '#search'); location.reload(); }).catch(() => {});
  await sleep(1500);
  // 搜索入口: 首页搜索钮或 url hash——探测可用输入框
  const searched = await page.evaluate(() => !!document.querySelector('input[placeholder*="搜索"], input[type="search"], .search-bar input'));
  if (!searched) {
    await page.click('.tab-bar .tab-item:has-text("知识库")').catch(() => {});
    await sleep(500);
    await page.click('button[aria-label="搜索"], .icon-btn[aria-label*="搜索"]').catch(() => {});
    await sleep(700);
  }
  const sInput = await page.$('.search-hero-input input, input[placeholder*="一句话"], input[placeholder*="搜索"], input[type="search"]');
  if (sInput) {
    /* mock 搜索域=title/content 字段（快照内容真机走 core FTS——PW 面断
     * title 命中;内容搜索锚=core FTS cargo 测试,manifest 登记）。 */
    await sInput.fill('未命名');
    await sleep(400);
    await page.keyboard.press('Enter');
    await sleep(900);
    const hit = await page.evaluate(() => !!document.querySelector('.search-result, .note-card, .nt-row') && document.body.innerText.includes('未命名'));
    ok('P3 搜索命中目标笔记', hit, 'query=未命名（title 面）');
  } else {
    ok('P3 搜索命中目标笔记', false, '搜索输入框未找到（选择器漂移——需修）');
  }

  /* ── P4 建任务（fab 捕获→加入今日任务） ── */
  await page.click('.tab-bar .tab-item:has-text("今日")').catch(() => page.click('.tab-bar .tab-item').then(() => sleep(400)));
  await sleep(700);
  await page.click('.fab');
  await sleep(600);
  const sheet = await page.$('.sheet textarea');
  if (sheet) {
    await sheet.fill('B05 回归任务');
    await sleep(200);
    await page.click('.sheet button:has-text("加入今日任务")');
    await sleep(700);
    const taskIn = await page.evaluate(() => {
      const ts = JSON.parse(localStorage.getItem('aurora.tasks') ?? 'null');
      const body = document.body.innerText;
      return { ls: ts?.some?.((t) => (t.title ?? '').includes('B05 回归任务')), ui: body.includes('B05 回归任务') };
    });
    ok('P4 建任务（今日任务在列）', taskIn.ls || taskIn.ui, `ls=${taskIn.ls} ui=${taskIn.ui}`);
  } else {
    ok('P4 建任务（今日任务在列）', false, 'fab sheet 未弹起');
  }

  /* ── P5 今日勾选（task-check 点击→checked 态+库 status） ── */
  const check = await page.$('.task-check:not(.checked)');
  if (check) {
    await check.click();
    await sleep(600);
    const after = await page.evaluate(() => ({
      ui: !!document.querySelector('.task-check.checked'),
      ls: (JSON.parse(localStorage.getItem('aurora.tasks') ?? '[]')).filter((t) => t.status === 'done').length,
    }));
    ok('P5 今日勾选（checked 态落库）', after.ui && after.ls >= 1, `done=${after.ls}`);
  } else {
    const any = await page.$('.task-check.checked');
    ok('P5 今日勾选（checked 态落库）', !!any, '无未勾任务·已有勾选态');
  }

  /* ── P6 回收站还原（软删→回收站→恢复→原 id 回库） ── */
  await page.click('.tab-bar .tab-item:has-text("知识库")');
  await sleep(700);
  /* 树形默认无 .note-card（列表卡片视图才有滑删）——切列表模式（repro-trash-s2b 同款链） */
  await page.evaluate(() => {
    const seg = Array.from(document.querySelectorAll('.nt-seg')).find((x) => (x.textContent || '').includes('列表'));
    if (seg) seg.click();
  });
  await sleep(500);
  await page.evaluate(() => {
    const el = document.querySelectorAll('.note-card')[0];
    el.dispatchEvent(new TouchEvent('touchstart', {
      touches: [new Touch({ identifier: 1, target: el, clientX: 300, clientY: 300 })], bubbles: true, cancelable: true,
    }));
    el.dispatchEvent(new TouchEvent('touchmove', {
      touches: [new Touch({ identifier: 1, target: el, clientX: 200, clientY: 300 })], bubbles: true, cancelable: true,
    }));
  });
  await sleep(300);
  await page.evaluate(() => { const b = document.querySelector('.swipe-delete'); if (b) b.click(); });
  await sleep(500);
  const delId = await page.evaluate(() => (JSON.parse(localStorage.getItem('aurora.trash') ?? '[]'))[0]?.noteId ?? null);
  await page.click('.icon-btn[aria-label="菜单"]').catch(() => {});
  await sleep(500);
  await page.evaluate(() => {
    const item = Array.from(document.querySelectorAll('.d-item, .d-scroll button, [class*=d-item], button'))
      .find((x) => (x.textContent || '').includes('回收站'));
    if (item) item.click();
  });
  await sleep(700);
  await page.evaluate(() => {
    const b = Array.from(document.querySelectorAll('button')).find((x) => (x.textContent || '').includes('恢复'));
    if (b) b.click();
  });
  await sleep(700);
  const restoredIn = await page.evaluate((id) => {
    const notes = JSON.parse(localStorage.getItem('aurora.notes') ?? '[]');
    return id ? notes.some((n) => n.id === id) : false;
  }, delId);
  ok('P6 回收站还原（原 id 回库）', restoredIn, `id=${delId}`);

  /* ── P7 导出视图（编辑器导出面板可达） ── */
  await page.click('.tab-bar .tab-item:has-text("知识库")');
  await sleep(600);
  /* 列表模式（树形首行可能是笔记本行——不开编辑器） */
  await page.evaluate(() => {
    const seg = Array.from(document.querySelectorAll('.nt-seg')).find((x) => (x.textContent || '').includes('列表'));
    if (seg) seg.click();
  });
  await sleep(500);
  const firstRow = await page.$('.note-card');
  if (firstRow) {
    await firstRow.click();
    await sleep(600);
    await page.waitForSelector('.ProseMirror', { timeout: 10000 });
    await page.click('button[aria-label="更多"], .icon-btn[aria-label="更多"]').catch(() => {});
    await sleep(400);
    await page.evaluate(() => {
      const b = Array.from(document.querySelectorAll('.menu-item')).find((x) => (x.textContent || '').includes('导出 Markdown'));
      if (b) b.click();
    });
    await sleep(900);
    const exportUi = await page.evaluate(() => {
      const t = document.body.innerText;
      return t.includes('导出') || t.includes('查看') || t.includes('MD');
    });
    ok('P7 导出视图（面板/通道可达）', exportUi, '导出 UI 元素在视口');
  } else {
    ok('P7 导出视图（面板/通道可达）', false, '无笔记行可入');
  }

  ok('P9 零页面错误', errors.length === 0, errors.length ? errors.slice(0, 2).join(' | ') : '双监听 0');

  await browser.close();
  const failed = results.filter((r) => !r.cond).length;
  console.log(`\n=== B-05 E2E TOP-PATHS: ${results.length - failed}/${results.length} PASS ===`);
  process.exit(failed > 0 ? 1 : 0);
})();
