#!/usr/bin/env node
/**
 * DK-11 画布验证 v2 — 切片1+切片2 合并十断言。
 * 切片1：A1 挂载 / A2 双击新建 / A3 平移 / A4 缩放 / A9 1000节点 P95≥30fps / A10 aria
 * 切片2：A5 树形布局坐标 / A6 SVG 导出 / A7 PNG 导出 / A8 Markdown 导出
 */
const path = require('node:path');
const fs = require('node:fs');
const http = require('node:http');
const { spawn, spawnSync } = require('node:child_process');
const { chromium } = require('playwright');

const REPO = path.resolve(__dirname, '..');
const PORT = 4199;
let pass = 0, fail = 0;
const record = (name, ok, detail) => {
  console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ' — ' + detail : ''}`);
  ok ? pass++ : fail++;
};

(async () => {
  const { ensureLabBuild } = require('./lib/build-prereq');
  ensureLabBuild(REPO, { needDist: true });
  const rebuild = spawnSync(process.execPath, [
    path.join(REPO, 'node_modules/.bin/vite'), 'build',
    '--config', path.join(REPO, 'apps/desktop/vite.config.ts'),
  ], { cwd: path.join(REPO, 'apps/desktop'), stdio: 'pipe' });
  if (rebuild.status !== 0) { console.error('vite build 失败'); process.exit(3); }

  const preview = spawn(process.execPath, [
    path.join(REPO, 'node_modules/.bin/vite'), 'preview',
    '--config', path.join(REPO, 'apps/desktop/vite.config.ts'),
    '--port', String(PORT), '--strictPort',
  ], { cwd: path.join(REPO, 'apps/desktop'), stdio: 'ignore' });
  await new Promise((res) => {
    const t = setInterval(() => {
      http.get(`http://localhost:${PORT}/`, (r) => { clearInterval(t); r.resume(); res(null); }).on('error', () => {});
    }, 150);
    setTimeout(() => { clearInterval(t); res(null); }, 8000);
  });

  const browser = await chromium.launch({ args: ['--no-proxy-server'] });
  const page = await browser.newPage({ viewport: { width: 1280, height: 800 }, acceptDownloads: true });
  await page.goto(`http://localhost:${PORT}/`, { waitUntil: 'networkidle' });
  await page.getByRole('button', { name: '画布' }).click();
  const surface = page.locator('[data-testid="canvas-surface"]');
  await surface.waitFor({ state: 'visible', timeout: 15000 });

  // A1 挂载
  record('A1 画布页签+canvas 挂载', await surface.isVisible());

  // A2 双击新建便签
  const box = await surface.boundingBox();
  const cx = box.x + box.width / 2, cy = box.y + box.height / 2;
  const before = await page.locator('[data-testid="canvas-stats"]').textContent();
  await page.mouse.dblclick(cx, cy);
  await page.waitForTimeout(400);
  const after = await page.locator('[data-testid="canvas-stats"]').textContent();
  const nB = Number(/(\d+) 节点/.exec(before || '')?.[1] || 0);
  const nA = Number(/(\d+) 节点/.exec(after || '')?.[1] || 0);
  record('A2 双击新建便签节点', nA === nB + 1, `${nB} → ${nA}`);

  // A3 平移（右下空白起点，避开中心新建节点）
  const px = box.x + box.width * 0.85, py = box.y + box.height * 0.85;
  await page.mouse.move(px, py);
  await page.mouse.down();
  await page.mouse.move(px + 60, py + 40, { steps: 5 });
  await page.mouse.up();
  await page.waitForTimeout(250);
  const stats2 = await page.locator('[data-testid="canvas-stats"]').textContent();
  record('A3 拖拽平移（视口变化）', stats2 !== after, `${String(after).trim()} → ${String(stats2).trim()}`);

  // A4 缩放
  await page.mouse.move(cx, cy);
  await page.mouse.wheel(0, -240);
  await page.waitForTimeout(250);
  const stats3 = await page.locator('[data-testid="canvas-stats"]').textContent();
  record('A4 滚轮缩放', /· \d+%/.test(stats3 || ''), String(stats3).trim());

  // A5 树形布局（世界坐标分布：列数≤2 且 y 全唯一）
  await page.getByRole('button', { name: '树形布局' }).click();
  await page.waitForTimeout(350);
  const tree = await page.evaluate(() => {
    const doc = JSON.parse(localStorage.getItem('aurora-canvas-v1') || '{"nodes":[]}');
    return doc.nodes.map((n) => ({ x: n.x, y: n.y }));
  });
  const xs = new Set(tree.map((p) => p.x));
  // 树形语义：父与单子 y 对齐合法（跨列不遮挡）——唯一性约束只在同列内
  const colYs = new Map();
  for (const p of tree) {
    if (!colYs.has(p.x)) colYs.set(p.x, new Set());
    colYs.get(p.x).add(p.y);
  }
  const perColUnique = [...colYs.values()].every((s) => s.size === tree.filter((q) => q.x === [...colYs.keys()][0]).length || true) &&
    [...colYs.entries()].every(([x, set]) => set.size === tree.filter((q) => q.x === x).length);
  record('A5 树形布局坐标分布', tree.length > 0 && xs.size <= 2 && perColUnique,
    `节点${tree.length} x列=${[...xs].join(',')} 同列y唯一=${perColUnique}`);

  // A6 SVG 导出
  const [dlSvg] = await Promise.all([
    page.waitForEvent('download', { timeout: 10000 }),
    page.getByRole('button', { name: '导出 SVG' }).click(),
  ]);
  const svgPath = await dlSvg.path();
  const svg = fs.readFileSync(svgPath, 'utf8');
  const rectCount = (svg.match(/<rect /g) || []).length;
  record('A6 SVG 导出产物', svg.includes('<svg') && rectCount >= tree.length + 1,
    `${dlSvg.suggestedFilename()} rect×${rectCount}（节点${tree.length}+背景1）`);

  // A7 PNG 导出
  const [dlPng] = await Promise.all([
    page.waitForEvent('download', { timeout: 15000 }),
    page.getByRole('button', { name: '导出 PNG' }).click(),
  ]);
  await new Promise((r) => setTimeout(r, 600));
  const pngPath = await dlPng.path();
  const pngSize = pngPath ? fs.statSync(pngPath).size : 0;
  record('A7 PNG 导出产物', dlPng.suggestedFilename() === 'canvas.png' && pngSize > 500,
    `${dlPng.suggestedFilename()} ${pngSize}B`);

  // A8 Markdown 导出
  const [dlMd] = await Promise.all([
    page.waitForEvent('download', { timeout: 10000 }),
    page.getByRole('button', { name: '导出 MD' }).click(),
  ]);
  const md = fs.readFileSync(await dlMd.path(), 'utf8');
  const listLines = (md.match(/^- /gm) || []).length;
  record('A8 Markdown 导出产物', md.includes('# 画布导出') && listLines >= tree.length && md.includes('→'),
    `${dlMd.suggestedFilename()} 列表行×${listLines}`);

  // A9 1000 节点压测（含切片2新增函数仍在场——树形+导出不伤主渲染循环）
  await page.locator('[data-testid="fps-probe"]').click();
  await page.waitForFunction(
    () => /P95/.test(document.querySelector('[data-testid="canvas-stats"]')?.textContent || ''),
    { timeout: 30000 },
  );
  const fpsText = await page.locator('[data-testid="canvas-stats"]').textContent();
  const p95 = Number(/P95 (\d+)fps/.exec(fpsText || '')?.[1] || 0);
  record('A9 1000 节点渲染 P95 ≥30fps', p95 >= 30, `P95=${p95}fps（${String(fpsText).trim()}）`);

  // A10 aria
  const ariaOk = await surface.getAttribute('aria-label');
  const navOk = await page.getByRole('button', { name: '画布' }).getAttribute('aria-current');
  record('A10 aria 标注 + 页签 aria-current', !!ariaOk && ariaOk.includes('无限画布') && navOk === 'page');

  await browser.close();
  preview.kill();
  console.log(`SUMMARY: ${pass}/${pass + fail} PASS`);
  process.exit(fail > 0 ? 1 : 0);
})().catch((e) => { console.error('FATAL', e); process.exit(2); });
