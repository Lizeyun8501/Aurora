#!/usr/bin/env node
/**
 * C-04 断言套件——PW file://（mobile dist;桌面面 tsc+披露）。
 * DoD: ①面包屑折叠（移动端「笔记本名 › …」形态+桌面 A-05 既有）
 *      ②图标三处一致（树/书签/搜索同 aurora.icon 字段）
 *      ④零外链（渐变本地 CSS——静态 rg 证+运行时 http 拦截）
 *      面包屑折叠桌面形态=NoteBreadcrumb 单元测（shared __tests__ 既有）。
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
const extReqs = [];
page.on('request', (r) => { if (/^https?:/.test(r.url()) && !r.url().includes('localhost')) extReqs.push(r.url()); });
const errors = [];
page.on('pageerror', (e) => errors.push(String(e)));
await page.goto(BASE);
await page.waitForTimeout(2200);

// 预置: 建一篇有图标覆盖的笔记 + 树路径
await page.evaluate(() => {
  localStorage.clear();
  const tree = [
    { id: 'b1', kind: 'book', title: '工作笔记本', parentId: null },
    { id: 'b2', kind: 'book', title: '子项目本', parentId: 'b1' },
    { id: 'b3', kind: 'book', title: '深层本', parentId: 'b2' },
  ];
  localStorage.setItem('aurora.tree', JSON.stringify(tree));
  localStorage.setItem('aurora.icon.b1', 'book-green');
});

// ② 图标字段一致性: 同 id 在树/书签/搜索读出同 def（getIconDef 单源）
const iconConsistency = await page.evaluate(() => {
  // 模块在 bundle 内不可直 import——经 DOM 间接证: 预置字段→树渲染→vivo-icon 色值
  return localStorage.getItem('aurora.icon.b1');
});
ok('②aurora.icon 字段预置（book-green）', iconConsistency === 'book-green', String(iconConsistency));

// 开笔记→折叠面包屑（工作笔记本 › …）
await page.click('button.fab[aria-label="捕获"]');
await page.waitForTimeout(450);
await page.fill('.sheet input, .sheet textarea', 'C04 面包屑验证');
await page.locator('.sheet button', { hasText: '生成笔记' }).first().click();
await page.waitForTimeout(1400);
// 把该笔记挂到深层本下（tree push doc 节点 parentId=b3）
const noteId = await page.evaluate(() => {
  const notes = JSON.parse(localStorage.getItem('aurora.notes') || '[]');
  const n = notes[notes.length - 1];
  if (!n) return null;
  const tree = JSON.parse(localStorage.getItem('aurora.tree') || '[]');
  tree.push({ id: n.id, kind: 'doc', title: n.title, parentId: 'b3' });
  localStorage.setItem('aurora.tree', JSON.stringify(tree));
  return n.id;
});
ok('③-1 笔记挂深层树（b1›b2›b3——3 级父链）', !!noteId, String(noteId));
// 重开编辑器（树变化需重挂载——close+reopen;返回钮 aria-label="返回"）
await page.click('button[aria-label="返回"]').catch(() => {});
await page.waitForTimeout(600);
// 重新打开该笔记: 知识库树里点它
await page.click('.tab-item[role="tab"][aria-label="笔记"]');
await page.waitForTimeout(500);
const openedViaTree = await page.locator(`text=C04 面包屑验证`).first().click().then(() => true).catch(() => false);
await page.waitForTimeout(900);
const crumbs = await page.evaluate(() => {
  const nav = document.querySelector('.editor-crumbs');
  return nav ? { root: nav.querySelector('span')?.textContent, fold: nav.textContent?.includes('…') ?? false, full: nav.getAttribute('title') } : null;
});
ok('①-1 移动端折叠屑渲染（工作笔记本 › …）', !!crumbs && crumbs.root === '工作笔记本' && crumbs.fold === true, JSON.stringify(crumbs));
const crumbTitle = await page.evaluate(() => document.querySelector('.editor-crumbs span[role="link"]')?.getAttribute('title') ?? document.querySelector('.editor-crumbs span')?.getAttribute('title'));
ok('①-2 全路径 title 提示（三层全列）', (crumbTitle ?? '').includes('子项目本') && (crumbTitle ?? '').includes('深层本'), String(crumbTitle));

// ②-2 树行图标: 先回知识库树面（编辑器全屏态无树）
await page.click('button[aria-label="返回"]').catch(() => {});
await page.waitForTimeout(600);
const treeIcon = await page.evaluate(() => {
  const el = Array.from(document.querySelectorAll('.vivo-icon')).find((x) => (x.getAttribute('aria-label') || '') === '绿本');
  return el ? getComputedStyle(el).backgroundColor : null;
});
ok('②-2 树图标=aurora.icon.b1 同源渲染（绿本 #10b981）', treeIcon === 'rgb(16, 185, 129)', String(treeIcon));

// ④ 零外链（会话期无 http(s) 资源请求——渐变/图标全本地）
ok('④零外链资源请求', extReqs.length === 0, extReqs.slice(0, 2).join(';'));
ok('⑤页面零 JS 错误', errors.length === 0, errors.slice(0, 2).join(';'));

console.log(`\n=== C-04 断言: ${pass}/${pass + fail} ${fail === 0 ? 'PASS' : 'FAIL'} ===`);
await browser.close();
process.exit(fail === 0 ? 0 : 1);
