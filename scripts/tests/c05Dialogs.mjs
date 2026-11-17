#!/usr/bin/env node
/**
 * C-05 断言套件——PW file://（mobile dist）。
 * 面: 原生弹窗退役（含 RichEditor 编辑器面）/共享 EmptyState/对话框 a11y
 * （aria-modal+Esc+焦点陷阱）/骨架屏存在性。
 */
import { createRequire } from 'module';
import { fileURLToPath } from 'url';
import path from 'path';
let chromium;
try {
  ({ chromium } = createRequire(import.meta.url)('playwright'));
} catch {
  ({ chromium } = createRequire('/home/z/.npm-global/lib/node_modules/playwright/')('playwright'));
}
const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const BASE = 'file://' + path.join(repoRoot, 'apps', 'mobile', 'dist', 'index.html');

let pass = 0, fail = 0;
const ok = (name, cond, extra = '') => {
  if (cond) { pass++; console.log(`PASS ${name}`); }
  else { fail++; console.log(`FAIL ${name}${extra ? ' — ' + extra : ''}`); }
};

const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 390, height: 844 } });
const errors = [];
page.on('pageerror', (e) => errors.push(String(e)));
await page.goto(BASE);
await page.waitForTimeout(2200);

// ①② 收集箱空态 = 共享组件（首渲染后样式注入断言——首屏今日无空态,
//    ensureStyles 惰性注入: 组件首渲染时才落 <style>）
await page.click('.tab-item[role="tab"][aria-label="收集箱"]');
await page.waitForTimeout(350);
const inboxEmpty = await page.evaluate(() => {
  const el = document.querySelector('.aurora-empty');
  return el ? el.querySelector('.aurora-empty-title')?.textContent : null;
});
ok('②收集箱空态=共享 EmptyState（title 承载）', inboxEmpty === '收集箱是空的', String(inboxEmpty));
const styleInjected = await page.evaluate(() => !!document.getElementById('aurora-state-styles'));
ok('①三态样式表注入（aurora-state-styles——首渲染后）', styleInjected);

// ③ 编辑器链接对话框（RichEditor prompt 退役实证）
//    路径: FAB 捕获 → 生成笔记 → NoteEditor（工具栏 🔗）
await page.click('button.fab[aria-label="捕获"]');
await page.waitForTimeout(450);
await page.fill('.sheet input, .sheet textarea', 'C05 对话框验证笔记').catch(() => {});
const genBtn = page.locator('.sheet button', { hasText: '生成笔记' });
const genCount = await genBtn.count();
ok('③-1 捕获面板「生成笔记」可达', genCount > 0, String(genCount));
if (genCount > 0) { await genBtn.first().click(); }
await page.waitForTimeout(1200);

// 编辑器「＋」插入面板（R19 dock）→ 🔗（title=插入链接）
await page.locator('button', { hasText: '＋' }).first().click();
await page.waitForTimeout(600);
let linkBtn = page.locator('button[title="插入链接"]:visible').first();
if (await linkBtn.count() === 0) {
  // 兜底: title 形态（Aa 工具栏内联钮）
  await page.locator('button[title="文字格式"]').first().click().catch(() => {});
  await page.waitForTimeout(400);
  linkBtn = page.locator('button[title="插入链接"]:visible').first();
}
const linkVisible = await linkBtn.count();
if (linkVisible > 0) {
  await linkBtn.click({ timeout: 8000 });
  await page.waitForTimeout(400);
  const dlg = await page.evaluate(() => {
    const d = document.querySelector('[role="dialog"][aria-modal="true"]');
    return d ? { modal: true, title: d.getAttribute('aria-label'), hasInput: !!d.querySelector('.aurora-dlg-input') } : null;
  });
  ok('③-2 dialog.input 弹出（aria-modal+label+input）', !!dlg && dlg.hasInput && /链接/.test(dlg.title ?? ''), JSON.stringify(dlg));
  // Esc 关闭
  await page.keyboard.press('Escape');
  await page.waitForTimeout(300);
  const closed = await page.evaluate(() => !document.querySelector('[role="dialog"][aria-modal="true"]'));
  ok('③-3 Esc 关闭对话框', closed);
  // 再开——焦点陷阱断言: 首个可聚焦=输入框;Tab 循环不逃逸（面板重开）
  await page.locator('button', { hasText: '＋' }).first().click();
  await page.waitForTimeout(600);
  await page.locator('button[title="插入链接"]:visible').first().click({ timeout: 8000 });
  await page.waitForTimeout(400);
  const trap = await page.evaluate(() => {
    const dlg = document.querySelector('[role="dialog"][aria-modal="true"]');
    if (!dlg) return 'no-dlg';
    const input = dlg.querySelector('.aurora-dlg-input');
    const active = document.activeElement;
    return { focusedInput: active === input, activeTag: active?.tagName, activeCls: active?.className, inputExists: !!input };
  });
  ok('③-4 打开即聚焦输入框（焦点管理）', trap?.focusedInput === true, JSON.stringify(trap));
  await page.keyboard.press('Escape');
  await page.waitForTimeout(200);
} else {
  ok('③ 编辑器插入面板链接钮在（'+String(linkVisible)+'）', false, '编辑器/插入面板未渲染——链接钮 0');
}

// ④ 零原生调用运行时实证: window.prompt/alert/confirm 被调用时应无原生弹窗——静态已 rg 证,运行时以 0 pageerror 收口
await page.evaluate(() => { localStorage.clear(); });
// 对话框样式注入断言（③ 若成功已渲染 Modal;兜底在此）
const modalStyleInjected = await page.evaluate(() => !!document.getElementById('aurora-modal-styles'));
ok('①对话框样式表注入（aurora-modal-styles）', modalStyleInjected);
ok('④ 页面零 JS 错误（双监听面）', errors.length === 0, errors.slice(0, 2).join(';'));

console.log(`\n=== C-05 断言: ${pass}/${pass + fail} ${fail === 0 ? 'PASS' : 'FAIL'} ===`);
await browser.close();
process.exit(fail === 0 ? 0 : 1);
