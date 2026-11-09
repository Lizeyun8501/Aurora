/**
 * E28 验证（TA 15:44 三令）
 * ① ⋮→删除→树行真消失（E27 漏断面——旧链树残留=「实际未删除」根因;本卡摘树+僵尸过滤双修）
 * ② 长按菜单「重命名」→sheet 输入→确认→树行+列表标题变
 * ③ pressing 按压反馈（pointerdown 加类/pointerup 释放——统一瞬态）
 * ④ 僵尸节点过滤（手造 tree 残留节点→不渲染）
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
  await page.click('.nt-arrow'); // 展开 book
  await page.waitForTimeout(400);
  await page.click('.nt-add'); // 子文档
  await page.waitForTimeout(800);

  // ===== ① ⋮ 删除→树行消失（关键新断言） =====
  const treeBefore = await page.evaluate(() => (JSON.parse(localStorage.getItem('aurora.tree') || '[]')).length);
  const docTitle = await page.$('.nt-row:not(.nt-book) .nt-title');
  await docTitle.click();
  await page.waitForSelector('.ProseMirror', { timeout: 10000 });
  await page.click('button[aria-label="更多"]');
  await page.waitForTimeout(500);
  await page.click('.menu-item:has-text("删除笔记")');
  await page.waitForTimeout(500);
  await page.click('.ctx-item.danger');
  await page.waitForTimeout(1200);
  const delState = await page.evaluate(() => ({
    treeLen: (JSON.parse(localStorage.getItem('aurora.tree') || '[]')).length,
    docRows: Array.from(document.querySelectorAll('.nt-row:not(.nt-book)')).length,
    treeShownDoc: Array.from(document.querySelectorAll('.nt-row:not(.nt-book) .nt-title')).map(b => b.textContent),
    trash: (JSON.parse(localStorage.getItem('aurora.trash') || '[]')).length,
  }));
  record('① ⋮删除→树节点真消失（localStorage+DOM 双断言）',
    delState.treeLen < treeBefore && delState.docRows === 0,
    `树 ${treeBefore}→${delState.treeLen} 行=${delState.docRows} trash=${delState.trash}`);

  // ===== ② 长按→重命名 =====
  await page.click('.nt-add'); await page.waitForTimeout(700);
  const row = await page.$('.nt-row:not(.nt-book)');
  const box = await row.boundingBox();
  const cdp = await page.context().newCDPSession(page);
  await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ x: box.x + 180, y: box.y + 20 }] });
  await page.waitForTimeout(600);
  await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
  await page.waitForTimeout(500);
  const menuItems = await page.evaluate(() => Array.from(document.querySelectorAll('.ctx-item')).map(b => b.textContent?.trim()));
  record('②a 长按菜单含重命名', menuItems.some(i => i?.includes('重命名')), JSON.stringify(menuItems));
  await page.click('.ctx-item:has-text("重命名")');
  await page.waitForTimeout(500);
  const rnState = await page.evaluate(() => ({
    sheet: !!document.querySelector('.rename-input'),
  }));
  record('②b 重命名 sheet 输入框', rnState.sheet, JSON.stringify(rnState));
  await page.fill('.rename-input', '改名后的笔记');
  await page.click('.ctx-item:has-text("确认")');
  await page.waitForTimeout(600);
  const afterRn = await page.evaluate(() => ({
    rowTitle: document.querySelector('.nt-row:not(.nt-book) .nt-title')?.textContent,
    overlay: Object.keys(localStorage).some(k => k.startsWith('aurora.title.')),
  }));
  record('②c 确认→树行显示新名+overlay 落盘', afterRn.rowTitle === '改名后的笔记' && afterRn.overlay, JSON.stringify(afterRn));

  // ===== ③ pressing 反馈（pointer 生命周期） =====
  const press1 = await page.evaluate(() => {
    const b = document.querySelector('.nt-row:not(.nt-book) .nt-title');
    b.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
    return { pressing: b.classList.contains('pressing') };
  });
  const press2 = await page.evaluate(() => {
    document.dispatchEvent(new PointerEvent('pointerup', { bubbles: true }));
    const b = document.querySelector('.nt-row:not(.nt-book) .nt-title');
    return { pressing: b.classList.contains('pressing') };
  });
  record('③ pressing down 加/up 清（保证释放）', press1.pressing && !press2.pressing, `${press1.pressing}→${press2.pressing}`);

  // ===== ④ 僵尸节点过滤（手造树残留） =====
  const zombieState = await page.evaluate(() => {
    const t = JSON.parse(localStorage.getItem('aurora.tree') || '[]');
    t.push({ id: 'note-zombie-999', kind: 'doc', title: '僵尸节点', parentId: t.find(x => x.kind === 'book')?.id ?? null });
    localStorage.setItem('aurora.tree', JSON.stringify(t));
    return { injected: t.length };
  });
  await page.click('.tab-bar .tab-item:has-text("今日")');
  await page.waitForTimeout(600);
  await page.click('.tab-bar .tab-item:has-text("知识库")');
  await page.waitForTimeout(900);
  const zAfter = await page.evaluate(() => ({
    zombieShown: Array.from(document.querySelectorAll('.nt-row .nt-title')).some(b => b.textContent?.includes('僵尸节点')),
    realShown: Array.from(document.querySelectorAll('.nt-row .nt-title')).some(b => b.textContent?.includes('改名后的笔记')),
  }));
  record('④ 僵尸节点不渲染（真笔记仍现）', zAfter.zombieShown === false && zAfter.realShown, JSON.stringify(zAfter));

  record('⑤ 零错误', errors.length === 0, errors.slice(0, 2).join('|') || 'clean');
  const passed = results.filter(r => r.p).length;
  console.log(`\n=== E28 删除真根+重命名+按压: ${passed}/${results.length} PASS ===`);
  await browser.close();
  process.exit(passed === results.length ? 0 : 1);
})();
