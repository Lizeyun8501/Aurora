/**
 * E21 验证（TA 21:57: VIVO 风格图标+更换）
 * ① 树行 VIVO 图标块（彩色圆角方非 emoji 裸文）
 * ② 点图标→选择器 sheet（8 格全集）
 * ③ 点选→toast+树行图标变色（持久）
 * ④ 重进知识库图标保持（overlay localStorage）
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

  await page.goto(BASE);
  await page.waitForTimeout(1600);
  await page.click('.tab-bar .tab-item:has-text("知识库")');
  await page.waitForTimeout(800);

  // ① 树行图标块
  const icon1 = await page.evaluate(() => {
    const btn = document.querySelector('.nt-icon-btn .vivo-icon');
    if (!btn) return null;
    const cs = getComputedStyle(btn);
    return { bg: cs.backgroundColor, radius: cs.borderRadius, glyph: btn.textContent?.slice(0, 2) };
  });
  record('① VIVO 图标块（彩色圆角方）', !!icon1 && icon1.radius === '6px' && icon1.bg !== 'rgba(0, 0, 0, 0)',
    `bg=${icon1?.bg} radius=${icon1?.radius} 字=${icon1?.glyph}`);

  // ② 点图标→sheet
  await page.click('.nt-icon-btn');
  await page.waitForTimeout(600);
  const picker = await page.evaluate(() => {
    const sheet = document.querySelector('.sheet');
    const cells = document.querySelectorAll('.vivo-icon-cell');
    const icons = document.querySelectorAll('.vivo-icon-cell .vivo-icon');
    return { hasSheet: !!sheet, cells: cells.length, icons: icons.length };
  });
  record('② 选择器 sheet（8 格）', picker.hasSheet && picker.cells === 8, `sheet=${picker.hasSheet} 格=${picker.cells}`);

  // ③ 点蓝本（第二格）→树行变蓝
  const bgBefore = await page.evaluate(() => getComputedStyle(document.querySelector('.nt-icon-btn .vivo-icon')).backgroundColor);
  await page.evaluate(() => {
    const cells = Array.from(document.querySelectorAll('.vivo-icon-cell'));
    cells[1]?.click();
  });
  await page.waitForTimeout(600);
  const after = await page.evaluate(() => ({
    bg: getComputedStyle(document.querySelector('.nt-icon-btn .vivo-icon')).backgroundColor,
    toast: document.querySelector('.toast')?.textContent ?? '',
    pickerClosed: !document.querySelector('.vivo-icon-cell'),
  }));
  record('③ 换图标→toast+树行变色', after.pickerClosed && /图标已更换/.test(after.toast) && after.bg !== bgBefore,
    `前=${bgBefore} 后=${after.bg} toast=${after.toast}`);

  // ④ 持久: 切 tab 回来
  await page.click('.tab-bar .tab-item:has-text("今日")');
  await page.waitForTimeout(700);
  await page.click('.tab-bar .tab-item:has-text("知识库")');
  await page.waitForTimeout(900);
  const persistBg = await page.evaluate(() => getComputedStyle(document.querySelector('.nt-icon-btn .vivo-icon')).backgroundColor);
  record('④ 图标 overlay 持久', persistBg === after.bg, `重进=${persistBg} 换后=${after.bg}`);

  record('⑤ 零错误', errors.length === 0, errors.slice(0, 2).join(' | ') || 'clean');
  const passed = results.filter((r) => r.p).length;
  console.log(`\n=== E21 VIVO 图标: ${passed}/${results.length} PASS ===`);
  await browser.close();
  process.exit(passed === results.length ? 0 : 1);
})();
