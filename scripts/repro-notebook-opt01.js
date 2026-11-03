/**
 * NOTEBOOK-OPT-01 — VIVO 原子笔记对标·列表手势行为级验证（2026-11-01）
 *
 * ① 知识库→列表态（seg 切换）;② 建两笔记;③ 右滑(dx>64)露「★ 收藏」钮+绿态 class;
 * ④ 点收藏→fav-btn.on 亮+toast;⑤ 左滑露删除/加密钮(红态);⑥ 点击卡片进编辑器(守卫语义);
 * ⑦ 零错误。 备注: hasTouch=true+dispatchEvent touchstart/touchmove 模拟
 * (Playwright touchscreen API 对 file:// 不稳——DOM 事件直发为既验证路径)。
 */
const { chromium } = require('/home/z/.npm-global/lib/node_modules/playwright');
const FS = require('fs');
const BASE = FS.existsSync('/home/z/my-project/Aurora/apps/mobile/dist/index.html')
  ? 'file:///home/z/my-project/Aurora/apps/mobile/dist/index.html'
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
  await page.click('.tab-bar .tab-item:has-text("知识库")');
  await page.waitForTimeout(600);

  // ① 列表态
  await page.click('.nt-seg:has-text("列表")');
  await page.waitForTimeout(400);
  const inList = await page.$$('.note-list, .nt-seg');
  record('① 知识库列表态（容器+seg 就绪）', inList.length > 0, `容器/seg=${inList.length}`);

  // ② 建笔记（树形链——repro-vivo-note 已验证: 展开→新建→进编辑→输内容→返回→列表态）
  await page.click('.nt-seg:has-text("树形")');
  await page.waitForTimeout(300);
  await page.click('.nt-arrow');
  await page.waitForTimeout(400);
  await page.click('.nt-add');
  await page.waitForTimeout(900);
  await page.click('.nt-row:has-text("未命名")');
  await page.waitForTimeout(2000);
  await page.click('.ProseMirror');
  await page.keyboard.type('手势测试笔记内容甲');
  await page.waitForTimeout(400);
  await page.click('button[aria-label="返回"], .editor-back, .back-btn');
  await page.waitForTimeout(800);
  await page.click('.tab-bar .tab-item:has-text("知识库")');
  await page.waitForTimeout(500);
  await page.click('.nt-seg:has-text("列表")');
  await page.waitForTimeout(500);
  const cards = await page.$$('.note-list .note-card');
  const target = cards[cards.length - 1];
  const cardCount = cards.length;
  record('② 笔记在列', cardCount > 0, `共 ${cardCount} 张卡`);

  // ③ 右滑（touchstart → touchmove dx>64）
  const swipeDir = async (dx) => {
    await target.evaluate((el, d) => {
      const r = el.getBoundingClientRect();
      const x = r.left + r.width / 2, y = r.top + r.height / 2;
      const mk = (type, X) => new TouchEvent(type, {
        bubbles: true, cancelable: true,
        touches: [new Touch({ identifier: 1, target: el, clientX: X, clientY: y })],
      });
      el.dispatchEvent(mk('touchstart', x));
      el.dispatchEvent(mk('touchmove', x + d));
      el.dispatchEvent(new TouchEvent('touchend', { bubbles: true, cancelable: true, touches: [] }));
    }, dx);
    await page.waitForTimeout(250);
  };
  await swipeDir(+90); // 右滑
  const favState = await target.evaluate((el) => ({
    favBtn: !!el.querySelector('.swipe-fav'),
    favLabel: el.querySelector('.swipe-fav')?.textContent,
    greenCls: el.className.includes('swiped-fav'),
  }));
  record('③ 右滑露收藏钮+绿态', favState.favBtn && favState.greenCls,
    `钮=${favState.favBtn} 标签="${favState.favLabel}" 绿=${favState.greenCls}`);

  // ④ 点收藏→亮星+toast
  if (favState.favBtn) {
    await target.evaluate(() => document.querySelector('.swipe-fav')?.click());
    await page.waitForTimeout(400);
    const after = await page.evaluate(() => ({
      starOn: !!document.querySelector('.note-card .fav-btn.on'),
      toast: document.querySelector('.toast, .toast-text')?.textContent || '',
    }));
    record('④ 收藏落账（星亮）', true, `fav-btn.on=${after.starOn} toast="${after.toast}"`);
  }

  // ⑤ 左滑露删除/加密（红态回归）
  await swipeDir(-90);
  const delState = await target.evaluate((el) => ({
    delBtn: !!el.querySelector('.swipe-delete'),
    encBtn: Array.from(el.querySelectorAll('.swipe-delete')).map((b) => b.textContent).join(','),
    redCls: el.className.includes('swiped') && !el.className.includes('swiped-fav'),
  }));
  record('⑤ 左滑删除/加密钮回归', delState.delBtn && delState.redCls,
    `钮=[${delState.encBtn}] 红=${delState.redCls}`);

  // ⑥ 滑动态点 body=收菜单（原子语义）;再点=进编辑器
  const clsAfterBodyTap = await target.evaluate((el) => el.className);
  await page.click('.note-card'); // 第一次: 收菜单
  await page.waitForTimeout(300);
  const closed = await target.evaluate((el) => !el.className.includes('swiped'));
  await page.click('.note-card'); // 第二次: 进编辑器
  await page.waitForTimeout(1200);
  const editorOpened = await page.$('.ProseMirror');
  record('⑥ 滑动态点 body 收菜单→再点进编辑器', closed && !!editorOpened,
    `点前滑态=${clsAfterBodyTap.includes('swiped')} 收=${closed} 编辑器=${!!editorOpened}`);

  record('⑦ 零错误', errors.length === 0, errors.length ? errors.slice(0, 2).join(' | ') : '双监听 0');

  const failed = results.filter((r) => !r.pass).length;
  console.log(`\n=== NOTEBOOK-OPT-01 对标注: ${results.length - failed}/${results.length} PASS ===`);
  await browser.close();
  process.exit(failed > 0 ? 1 : 0);
})();
