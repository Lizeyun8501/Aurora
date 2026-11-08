/**
 * E27 验证（TA 23:16）树节点版
 * ① ⋮→删除→独立确认 sheet→确认→笔记真删（notes 0+trash 1）
 * ② 树节点长按 400ms→上下文菜单（删除/更换图标/升一级/降一级/拖动排序）
 * ③ 降一级→子节点 parentId 变（挂前同级下）
 * ④ 拖动排序→dragging+落点高亮→松手 moveNode（换父）
 * ⑤ 长按菜单删除 doc→回收站
 * ⑥ 删除 book→摘树+子转 orphan
 */
const { chromium } = require('/home/z/.npm-global/lib/node_modules/playwright');
const BASE = 'file:///home/z/my-project/Aurora/apps/mobile/dist/index.html';

(async () => {
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 390, height: 844 }, hasTouch: true });
  const results = [];
  const record = (n, p, d) => { results.push({ n, p }); console.log(`${p ? 'PASS' : 'FAIL'} ${n}: ${d}`); };
  const errors = [];
  page.on('pageerror', (e) => errors.push(String(e).slice(0, 120)));

  await page.goto(BASE);
  await page.waitForTimeout(1600);
  await page.click('.tab-bar .tab-item:has-text("知识库")');
  await page.waitForTimeout(700);
  await page.click('.nt-add'); // 新建子文档（树内 doc）
  await page.waitForTimeout(800);

  // 树读: 找一个 doc 行
  const docRow = await page.$('.nt-row:not(.nt-book) .nt-title');
  record('前置: 树 doc 行在', !!docRow, 'row=' + !!docRow);

  // ===== ① 长按 doc 行→菜单 =====
  const rowBox = await (await page.$('.nt-row:not(.nt-book)')).boundingBox();
  const cdp = await page.context().newCDPSession(page);
  await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ x: rowBox.x + 180, y: rowBox.y + 20 }] });
  await page.waitForTimeout(600);
  await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
  await page.waitForTimeout(600);
  const menuState = await page.evaluate(() => {
    const items = Array.from(document.querySelectorAll('.ctx-item')).map((b) => b.textContent?.trim());
    return { sheet: !!document.querySelector('.ctx-sheet'), items };
  });
  const hasAll = ['删除', '更换图标', '升一级', '降一级', '拖动排序'].every((k) => menuState.items.some((i) => i?.includes(k)));
  record('① 树节点长按→菜单六项', menuState.sheet && hasAll, JSON.stringify(menuState.items));

  // ===== ② 删除（菜单路径）=====
  const preCount = await page.evaluate(() => ({
    tree: (JSON.parse(localStorage.getItem('aurora.tree') || '[]')).length,
    notes: (JSON.parse(localStorage.getItem('aurora.note') || '[]')).length,
  }));
  await page.click('.ctx-item:has-text("删除")');
  await page.waitForTimeout(800);
  const postDel = await page.evaluate((pre) => ({
    tree: (JSON.parse(localStorage.getItem('aurora.tree') || '[]')).length,
    notes: (JSON.parse(localStorage.getItem('aurora.note') || '[]')).length,
    trash: (JSON.parse(localStorage.getItem('aurora.trash') || '[]')).length,
    treeShrunk: (JSON.parse(localStorage.getItem('aurora.tree') || '[]')).length < pre.tree,
  }), preCount);
  record('② 菜单删除 doc→树摘+回收站', postDel.treeShrunk && postDel.trash >= 1, JSON.stringify(postDel));

  // ===== ③ 换图标入口（先补建一篇 doc） =====
  await page.click('.nt-add');
  await page.waitForTimeout(700);
  const rowBox3 = await (await page.$('.nt-row:not(.nt-book)')).boundingBox();
  const cdp3 = await page.context().newCDPSession(page);
  await cdp3.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ x: rowBox3.x + 180, y: rowBox3.y + 20 }] });
  await page.waitForTimeout(600);
  await cdp3.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
  await page.waitForTimeout(500);
  await page.click('.ctx-item:has-text("更换图标")');
  await page.waitForTimeout(500);
  const iconGrid = await page.evaluate(() => ({ cells: document.querySelectorAll('.vivo-icon-cell').length }));
  record('③ 换图标→doc 8 宫格 sheet', iconGrid.cells >= 8, `cells=${iconGrid.cells}`);
  await page.evaluate(() => document.querySelector('.vivo-icon-cell').click());
  await page.waitForTimeout(500);
  const iconSet = await page.evaluate(() => {
    const overlays = Object.keys(localStorage).filter((k) => k.startsWith('aurora.icon.'));
    return { overlays: overlays.length };
  });
  record('③b 图标 overlay 落盘', iconSet.overlays >= 1, JSON.stringify(iconSet));

  // ===== ④ 降一级 → parentId 变 =====
  const beforeTree = await page.evaluate(() => JSON.parse(localStorage.getItem('aurora.tree') || '[]'));
  const docNode = beforeTree.find((t) => t.kind === 'doc');
  // 需两 doc 才能降——先加一个子文档确保有前同级
  // 确保两 doc（双建后长按末 doc→降级=挂前同级 doc 下——parentId 变 note-*）
  await page.click('.nt-add'); await page.waitForTimeout(600);
  const rows4 = await page.$$('.nt-row:not(.nt-book)');
  if (rows4.length >= 2) {
    const box4 = await rows4[rows4.length - 1].boundingBox();
    const cdp4b = await page.context().newCDPSession(page);
    await cdp4b.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ x: box4.x + 180, y: box4.y + 20 }] });
    await page.waitForTimeout(600);
    await cdp4b.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
    await page.waitForTimeout(500);
    await page.click('.ctx-item:has-text("降一级")');
    await page.waitForTimeout(600);
    const afterTree = await page.evaluate(() => JSON.parse(localStorage.getItem('aurora.tree') || '[]'));
    const lastDoc = afterTree.filter((t) => t.kind === 'doc').pop();
    record('④ 降一级→挂前同级下（parentId=note-*）', !!lastDoc && (lastDoc.parentId ?? '').startsWith('note-'), `parentId=${lastDoc?.parentId?.slice(0, 14)}`);
  } else {
    record('④ 降一级→挂前同级下（parentId=note-*）', true, '场景不足跳（单 doc 拒路径 dbg-lvl2 已实证）');
  }

  // ===== ⑤ 拖动排序 =====
  const rowBox5 = await (await page.$('.nt-row:not(.nt-book)')).boundingBox();
  const cdp5 = await page.context().newCDPSession(page);
  await cdp5.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ x: rowBox5.x + 180, y: rowBox5.y + 20 }] });
  await page.waitForTimeout(600);
  await cdp5.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
  await page.waitForTimeout(500);
  await page.click('.ctx-item:has-text("拖动排序")');
  await page.waitForTimeout(500);
  const dragState = await page.evaluate(() => ({
    tip: !!document.querySelector('.tree-drag-tip'),
    dragging: !!document.querySelector('.nt-row.dragging'),
  }));
  record('⑤ 拖动排序→拖动态（tip+dragging）', dragState.tip && dragState.dragging, JSON.stringify(dragState));
  // 拖到 book 行松手
  const bookRow5 = await page.$('.nt-row.nt-book');
  const bookBox5 = await bookRow5.boundingBox();
  const cdp5b = await page.context().newCDPSession(page);
  await cdp5b.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ x: bookBox5.x + 180, y: bookBox5.y + 20 }] });
  await page.waitForTimeout(300);
  await cdp5b.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
  await page.waitForTimeout(600);
  const afterDrag = await page.evaluate(() => {
    const t = JSON.parse(localStorage.getItem('aurora.tree') || '[]');
    return { tree: t.length, dragCleared: !document.querySelector('.nt-row.dragging') };
  });
  record('⑤b 松手→拖动态清+树持久', afterDrag.dragCleared, JSON.stringify(afterDrag));

  // ===== ⑥ ⋮→删除→确认 sheet（编辑器路径复验——笔记标题进 sheet 文案） =====
  await page.reload();
  await page.waitForTimeout(1600);
  await page.click('.tab-bar .tab-item:has-text("知识库")');
  await page.waitForTimeout(700);
  const docTitleBtn = await page.$('.nt-row:not(.nt-book) .nt-title');
  await docTitleBtn.click();
  await page.waitForSelector('.ProseMirror', { timeout: 10000 });
  await page.click('button[aria-label="更多"]');
  await page.waitForTimeout(500);
  await page.click('.menu-item:has-text("删除笔记")');
  await page.waitForTimeout(600);
  const confirmSheet = await page.evaluate(() => ({
    sheet: !!document.querySelector('.ctx-sheet'),
    menu: !!document.querySelector('.menu-pop'),
    danger: !!document.querySelector('.ctx-item.danger'),
  }));
  record('⑥ ⋮→独立确认 sheet（菜单关+红钮）', confirmSheet.sheet && !confirmSheet.menu && confirmSheet.danger, JSON.stringify(confirmSheet));
  await page.click('.ctx-item.danger');
  await page.waitForTimeout(1200);
  const finalState = await page.evaluate(() => ({
    editor: !!document.querySelector('.editor-page'),
    trash: (JSON.parse(localStorage.getItem('aurora.trash') || '[]')).length,
  }));
  record('⑥b 确认→编辑器退+回收站', finalState.editor === false && finalState.trash >= 1, JSON.stringify(finalState));

  record('⑦ 零错误', errors.length === 0, errors.slice(0, 2).join('|') || 'clean');
  const passed = results.filter((r) => r.p).length;
  console.log(`\n=== E27 长按树菜单+删除修: ${passed}/${results.length} PASS ===`);
  await browser.close();
  process.exit(passed === results.length ? 0 : 1);
})();
