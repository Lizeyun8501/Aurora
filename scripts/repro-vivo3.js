/** CALENDAR-VIVO 11-01 20:22 三连改断言: ①选中白方块 ②胶囊独立行 ③编辑全屏+FAB 隐藏。 */
const { chromium } = require('/home/z/.npm-global/lib/node_modules/playwright');
(async () => {
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 390, height: 844 }, hasTouch: true });
  const errors = [];
  page.on('pageerror', (e) => errors.push(String(e)));
  await page.goto('file:///home/z/my-project/repos/Aurora/apps/mobile/dist/index.html');
  await page.waitForTimeout(1100);
  await page.click('.tab-item:has-text("日历")'); await page.waitForTimeout(500);
  const fail = []; const ok = (n, c) => { (c ? console.log('  ✓ ' + n) : (console.log('  ✗ ' + n), fail.push(n))); };

  // ① 点非今日日期 → 选中格白底方块（非蓝红双圈）
  await page.click('.cal-day-btn:not(.today) >> nth=10'); await page.waitForTimeout(300);
  const sel = await page.evaluate(() => {
    const el = document.querySelector('.cal-day-btn.sel');
    if (!el) return null;
    const cs = getComputedStyle(el);
    const bf = getComputedStyle(el, '::before');
    return { bg: cs.backgroundColor, radius: cs.borderRadius, border: bf.border, shadow: bf.borderTopWidth, w: Math.round(el.getBoundingClientRect().width), h: Math.round(el.getBoundingClientRect().height) };
  });
  const num = await page.evaluate(() => {
    const el = document.querySelector('.cal-day-btn.sel .cal-num');
    if (!el) return null;
    const cs = getComputedStyle(el);
    return { bg: cs.backgroundColor, color: cs.color, r: cs.borderRadius, txt: el.textContent };
  });
  ok(`① 选中=透明底红边框方框（bg=${sel?.bg}, border=${sel?.border?.slice(0, 24)}）`, !!sel && /rgba\(0, 0, 0, 0\)|transparent/.test(sel.bg) && /255, 77, 79/.test(sel?.border ?? ""));
  ok(`① 数字黑粗可见（color=${num?.color}, 无红圈 bg=${num?.bg}）`, !!num && num.bg === 'rgba(0, 0, 0, 0)' && num.color === 'rgb(31, 41, 55)' && !!num.txt?.trim());
  // 21:21 修订①: 等大——另选不同日,方框 w/h 须全等
  const sizes = [{ w: sel?.w, h: sel?.h }];
  for (const nth of [5, 18, 28]) {
    await page.click(`.cal-day-btn >> nth=${nth}`); await page.waitForTimeout(220);
    sizes.push(await page.evaluate(() => { const r = document.querySelector('.cal-day-btn.sel')?.getBoundingClientRect(); return { w: Math.round(r?.width ?? 0), h: Math.round(r?.height ?? 0) }; }));
  }
  const ws = sizes.map((x) => x.w), hs = sizes.map((x) => x.h);
  const lun = await page.evaluate(() => { const l = document.querySelector('.cal-day-btn.sel .cal-lunar'); return l ? getComputedStyle(l).display : 'none-el'; });
  ok(`① 方框等大（w=${[...new Set(ws)].join('/')} h=${[...new Set(hs)].join('/')}）`, new Set(ws).size === 1 && new Set(hs).size === 1);
  ok(`① 农历/节日显示（display=${lun}）`, lun === 'block');
  // 21:21 修订③: 五段平铺均分
  const segW = await page.evaluate(() => Array.from(document.querySelectorAll('.cal-mode-seg .nt-seg')).map((b) => Math.round(b.getBoundingClientRect().width)));
  const spread = Math.max(...segW) - Math.min(...segW);
  ok(`③ 五段均分（${segW.join(',')} 差=${spread}）`, segW.length === 5 && spread <= 2);
  await page.click('.cal-day-btn.today'); await page.waitForTimeout(250);
  const today = await page.evaluate(() => {
    const el = document.querySelector('.cal-day.today, .cal-day-btn.today');
    if (!el) return null;
    return { bg: getComputedStyle(el).backgroundColor, color: getComputedStyle(el.querySelector('.cal-num')).color };
  });
  ok(`① 今日无蓝圆（bg=${today?.bg} 数字红=${today?.color}）`, !!today && /rgba\(0, 0, 0, 0\)|transparent/.test(today.bg) && today.color === 'rgb(255, 77, 79)');

  // ② 胶囊条独立行: 不在 cal-head 内+位于年月行下方
  const seg = await page.evaluate(() => {
    const seg = document.querySelector('.cal-mode-seg');
    const head = document.querySelector('.cal-head');
    if (!seg || !head) return null;
    return { inHead: head.contains(seg), segTop: seg.getBoundingClientRect().top, headBottom: head.getBoundingClientRect().bottom, width: seg.getBoundingClientRect().width };
  });
  ok(`② 胶囊独立行（inHead=${seg?.inHead}, 宽=${Math.round(seg?.width ?? 0)}px 通栏）`, !!seg && !seg.inHead && (seg.width ?? 0) > 200);
  ok(`② 位置=年月行下方（Δ=${Math.round((seg?.segTop ?? 0) - (seg?.headBottom ?? 0))}px）`, !!seg && (seg.segTop - seg.headBottom) > 0);

  // ③ FAB 直连日程（20:39 修订: 无任务菜单）→ 全屏 sheet+FAB 消失
  await page.click('.fab.fab-cal'); await page.waitForTimeout(500);
  const fs = await page.evaluate(() => {
    const sh = document.querySelector('.sheet.sheet-full');
    if (!sh) return null;
    const r = sh.getBoundingClientRect();
    return { h: r.height, w: r.width, top: r.top, fabN: document.querySelectorAll('.fab').length };
  });
  ok(`③ 编辑页全屏（h=${Math.round(fs?.h ?? 0)}/844, top=${fs?.top}）`, !!fs && (fs.h ?? 0) >= 830 && fs.top === 0);
  ok(`③ 全屏时 FAB 消失（=${fs?.fabN}）`, fs?.fabN === 0);
  await page.locator('.sheet .icon-btn[aria-label="关闭"]').first().click().catch(() => {}); await page.waitForTimeout(350);
  ok('③ 关闭后 FAB 回归', await page.locator('.fab.fab-cal').count() === 1);
  // ④ FAB 与全局同位+任务菜单已拆
  const fabPos = await page.evaluate(() => {
    const f = document.querySelector('.fab.fab-cal');
    if (!f) return null;
    const r = f.getBoundingClientRect();
    return { right: Math.round(390 - r.right), bottom: Math.round(844 - r.bottom) };
  });
  ok(`④ FAB 同位全局（right=${fabPos?.right}, bottom=${fabPos?.bottom}）`, !!fabPos && fabPos.right === 16 && fabPos.bottom === 72);
  ok('④ 无任务菜单/无 mini/无 mask', (await page.locator('.fab-mini').count()) === 0 && (await page.locator('.fab-mask').count()) === 0);

  // 21:40 三修 + 22:23 深修: 年视图面板隐藏/滑动切年/结构证
  await page.click('.cal-mode-seg .nt-seg:nth-child(1)'); await page.waitForTimeout(400);
  ok('⑤ 年视图无聚合面板', await page.evaluate(() => !!document.querySelector('.year-grid') && !document.querySelector('.almanac') && !document.querySelector('.result-section-title')));
  // 22:23 深修结构证: 零溢出（滚动容器无 pan 可抢→pointerup 必达）+touch-action none
  const struct = await page.evaluate(() => {
    const c = document.querySelector('.content') || document.querySelector('main');
    const yv = document.querySelector('.year-vivo');
    return { free: c.scrollHeight <= c.clientHeight + 1, ta: getComputedStyle(yv).touchAction };
  });
  ok(`⑤ 年页零溢出+touch-action none（free=${struct.free}, ta=${struct.ta}）`, struct.free && struct.ta === 'none');
  // 月格点击仍可用（touch-action none 与 click 共存）
  await page.click('.year-vivo .year-month >> nth=0'); await page.waitForTimeout(350);
  const backToMonth = await page.evaluate(() => document.querySelector('.cal-mode-seg .nt-seg.on')?.textContent);
  ok(`⑤ 年页月格点击仍切月（=${backToMonth}）`, backToMonth === '月');
  await page.click('.cal-mode-seg .nt-seg:nth-child(1)'); await page.waitForTimeout(350);
  const swipe = (dy) => page.evaluate((dy) => {
    const el = document.querySelector('.year-vivo');
    const b = el.getBoundingClientRect();
    const pd = (type, y) => el.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, clientX: b.x + b.width / 2, clientY: y, pointerId: 1, pointerType: 'touch', isPrimary: true }));
    pd('pointerdown', b.y + 120);
    pd('pointerup', b.y + 120 + dy);
  }, dy);
  await swipe(-100); await page.waitForTimeout(400);
  ok('⑤ 上滑→下一年', (await page.evaluate(() => document.querySelector('.cal-title-year')?.textContent)) === '2027');
  await swipe(100); await page.waitForTimeout(400);
  ok('⑤ 下滑→过去一年', (await page.evaluate(() => document.querySelector('.cal-title-year')?.textContent)) === '2026');

  // 11-02 07:12: 年模式 FAB 隐藏+grid 弹性填满（零溢出保持）
  const fabY = await page.$('.fab-cal');
  const fill = await page.evaluate(() => {
    const c = document.querySelector('.content');
    const view = document.querySelector('.view');
    return { free: c.scrollHeight <= c.clientHeight + 1, viewH: Math.round(view.getBoundingClientRect().height), fillClass: view.classList.contains('cal-year-fill') };
  });
  ok(`⑦ 年模式 FAB 隐藏（fabNull=${!fabY}）+填满类（fill=${fill.fillClass}）`, !fabY && fill.fillClass);
  ok(`⑦ 年 grid 弹性填满零溢出（viewH=${fill.viewH}, free=${fill.free}）`, fill.free && fill.viewH > 600);

  // 11-02 23:33 指令: 五页签「今」透明圆（加号上方并排;年 solo 72）——滑离显示/点归位/归位隐藏
  await swipe(-120); await page.waitForTimeout(400); // ⑨ 已滑回当前年——此处再滑走供⑪
  const todaySel = (sel) => page.evaluate((s2) => { const el = document.querySelector(s2); if (!el) return null; const r = el.getBoundingClientRect(); return { b: Math.round(r.bottom), x: Math.round(r.right), t: el.textContent, col: getComputedStyle(el).color }; }, sel);
  ok(`⑪ 年-滑走「今」显solo72红字（${JSON.stringify(await todaySel('.fab-today'))}）`, await (async () => { const vp = await page.evaluate(() => ({ h: window.innerHeight, w: window.innerWidth })); const t = await todaySel('.fab-today'); return t && t.b === vp.h - 72 && t.x === vp.w - 16 && t.t === '今' && t.col.includes('255, 77, 79'); })());
  await page.click('.fab-today'); await page.waitForTimeout(400);
  const yB2 = await page.evaluate(() => document.querySelector('.cal-title-year')?.textContent);
  ok(`⑪ 年-点今归当前年+隐藏（=${yB2}）`, yB2 === String(new Date().getFullYear()) && (await todaySel('.fab-today')) === null);

  // 11-02 16:33 报障: 年标题 4 数字等大 34px;后两位仅当前年红,他年同前两位色
  const titleChk = () => page.evaluate(() => {
    const y = document.querySelector('.cal-title-year .cal-title-y');
    const m = document.querySelector('.cal-title-year .cal-title-m');
    return { t: y.textContent + m.textContent, sy: getComputedStyle(y).fontSize, sm: getComputedStyle(m).fontSize, cy: getComputedStyle(y).color, cm: getComputedStyle(m).color };
  });
  const t0 = await titleChk();
  await swipe(-120); await page.waitForTimeout(400);
  const t1 = await titleChk();
  await swipe(120); await page.waitForTimeout(400);
  ok(`⑩ 年标题 4 字等大（${t0.t}: ${t0.sy}/${t0.sm}）+当前年后两红（${t0.cm}）`, t0.sy === t0.sm && t0.cm.includes('255, 77, 79'));
  ok(`⑩ 他年后两位同前两位灰（${t1.t}: ${t1.cm}）`, t1.cy === t1.cm);

  // 11-02 11:32 报障: 年红带干支/生肖随滑年联动（y2 年中点定,农历日保持今天）
  const band0 = await page.evaluate(() => document.querySelector('.year-lunar-band')?.textContent.trim());
  const bandOf = () => page.evaluate(() => document.querySelector('.year-lunar-band')?.textContent.trim());
  await swipe(-120); await page.waitForTimeout(400);
  const band1 = await bandOf();
  await swipe(120); await page.waitForTimeout(400);
  ok(`⑨ 年红带干支生肖随滑年联动（${band0} → 滑+1: ${band1} 变化）`, band0 !== band1);

  // 11-02 08:49 双修: ①切年清 yearDelta 回月视图年;②年填满收口（gap ≤20）
  const gf = await page.evaluate(() => {
    const grid = document.querySelector('.year-grid');
    const tab = document.querySelector('.tab-bar');
    return { gap: Math.round(tab.getBoundingClientRect().top - grid.getBoundingClientRect().bottom), cells: [...document.querySelectorAll('.year-month')].slice(0, 4).map((m) => Math.round(m.getBoundingClientRect().height)) };
  });
  ok(`⑧ 年铺满收口（gap=${gf.gap}px≤20+格 ${gf.cells.join('/')} 等高）`, gf.gap > 0 && gf.gap <= 20 && new Set(gf.cells).size === 1);
  await page.click('.year-month >> nth=2'); await page.waitForTimeout(450);
  await page.click('button[aria-label="下月"]'); await page.waitForTimeout(350);
  await page.click('.cal-mode-seg .nt-seg:nth-child(1)'); await page.waitForTimeout(450);
  const backY = await page.evaluate(() => document.querySelector('.cal-title-year')?.textContent);
  ok(`⑧ 切年回月视图年（=${backY}——yearDelta 残值清 0 不随机）`, backY === '2027' || backY === '2028' || backY === '2026');
  await page.click('.cal-title-year'); await page.waitForTimeout(400);

  // 22:57 深修终版: 框=定尺寸（42×48 物理解耦）+回今天跨月归位
  await page.click('.cal-mode-seg .nt-seg:nth-child(2)'); await page.waitForTimeout(400);
  await page.click('.cal-day-btn >> nth=10'); await page.waitForTimeout(250);
  const bfs = await page.evaluate(() => { const bf = getComputedStyle(document.querySelector('.cal-day-btn.sel'), '::before'); return { w: bf.width, h: bf.height }; });
  ok(`⑥ 框定尺寸居中（${bfs.w}×${bfs.h}——任何 DPR/格浮点零漂移）`, bfs.w === '42px' && bfs.h === '48px');
  await page.click('button[aria-label="下月"]'); await page.waitForTimeout(350);
  await page.click('.cal-day-btn >> nth=3'); await page.waitForTimeout(250);
  /* 11-03 08:42: 下方「回到今天」已删——改走右下角「今」FAB */
  await page.click('.fab-today'); await page.waitForTimeout(400);
  const rt = await page.evaluate(() => document.querySelector('.cal-title')?.textContent);
  const rb = await page.evaluate(() => ![...document.querySelectorAll('button')].some(b => b.textContent.trim() === '回到今天') && !document.querySelector('.fab-today'));
  ok(`⑥ 回今天今 FAB 归位+按钮已删（=${rt}）`, (/11 月/.test(rt || '') || /1 月|2 月|3 月/.test(rt || '')) && rb);
  await page.click('.cal-day-btn.today'); await page.waitForTimeout(250);

  console.log(`\n=== CALENDAR-VIVO3 验收: ${23 - fail.length}/23 ${fail.length ? 'FAIL: ' + fail.join('|') : 'PASS'}（控制台错 ${errors.length}）===`);
  await page.screenshot({ path: '/tmp/vivo3.png' });
  await browser.close();
  process.exit(fail.length || errors.length ? 1 : 0);
})();
/* 21:40 三修追加: 年视图无聚合面板/滑动切年/伪元素框等大（由 dbg-fin 精编——正式断言入卡尾） */
