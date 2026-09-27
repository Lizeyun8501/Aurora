#!/usr/bin/env node
/**
 * DK-11 画布第一切片验证 — 六断言（dk05m_verify 同构模式）。
 * 前置：ensureLabBuild（D1 基建）+ 主 dist 无条件重建（build-prereq 源监测面不含组件文件）。
 * 断言：A1 画布页签挂载 / A2 双击新建便签 / A3 拖拽平移视口变化 /
 *       A4 滚轮缩放 / A5 1000 节点压测 P95 ≥30fps / A6 aria 可访问面。
 */
const path = require('node:path');
const { spawn, spawnSync } = require('node:child_process');
const http = require('node:http');
const { chromium } = require('playwright');

const REPO = path.resolve(__dirname, '..');
const PORT = 4199;
let pass = 0, fail = 0;
const record = (name, ok, detail) => {
  console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ' — ' + detail : ''}`);
  ok ? pass++ : fail++;
};

(async () => {
  // D1 基建：editorlab+dist 陈旧/缺失自动重建
  const { ensureLabBuild } = require('./lib/build-prereq');
  ensureLabBuild(REPO, { needDist: true });
  // build-prereq 源监测面（editor-lab/schema）不含 desktop 组件文件——本卡组件源变化需无条件重建主 dist
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
  const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
  await page.goto(`http://localhost:${PORT}/`, { waitUntil: 'networkidle' });
  await page.getByRole('button', { name: '画布' }).click();
  const surface = page.locator('[data-testid="canvas-surface"]');
  await surface.waitFor({ state: 'visible', timeout: 15000 });

  // A1 挂载
  record('A1 画布页签+canvas 元素挂载', await surface.isVisible());

  // A2 双击新建便签（原生 dblclick）
  const box = await surface.boundingBox();
  const cx = box.x + box.width / 2, cy = box.y + box.height / 2;
  const before = await page.locator('[data-testid="canvas-stats"]').textContent();
  await page.mouse.dblclick(cx, cy);
  await page.waitForTimeout(400);
  const after = await page.locator('[data-testid="canvas-stats"]').textContent();
  const nB = Number(/(\d+) 节点/.exec(before || '')?.[1] || 0);
  const nA = Number(/(\d+) 节点/.exec(after || '')?.[1] || 0);
  record('A2 双击新建便签节点', nA === nB + 1, `${nB} → ${nA}`);

  // A3 拖拽平移（视口坐标变化）——起点避开 A2 中心新建节点（右下空白区）
  const px = box.x + box.width * 0.85, py = box.y + box.height * 0.85;
  await page.mouse.move(px, py);
  await page.mouse.down();
  await page.mouse.move(px + 60, py + 40, { steps: 5 });
  await page.mouse.up();
  await page.waitForTimeout(250);
  const stats2 = await page.locator('[data-testid="canvas-stats"]').textContent();
  record('A3 拖拽平移（视口变化）', stats2 !== after, `${String(after).trim()} → ${String(stats2).trim()}`);

  // A4 滚轮缩放
  await page.mouse.move(cx, cy);
  await page.mouse.wheel(0, -240);
  await page.waitForTimeout(250);
  const stats3 = await page.locator('[data-testid="canvas-stats"]').textContent();
  record('A4 滚轮缩放', /· \d+%/.test(stats3 || ''), String(stats3).trim());

  // A5 1000 节点压测
  await page.locator('[data-testid="fps-probe"]').click();
  await page.waitForFunction(
    () => /P95/.test(document.querySelector('[data-testid="canvas-stats"]')?.textContent || ''),
    { timeout: 30000 },
  );
  const fpsText = await page.locator('[data-testid="canvas-stats"]').textContent();
  const m = /P95 (\d+)fps/.exec(fpsText || '');
  const p95 = m ? Number(m[1]) : 0;
  record('A5 1000 节点渲染 P95 ≥30fps', p95 >= 30, `P95=${p95}fps（${String(fpsText).trim()}）`);

  // A6 aria
  const ariaOk = await surface.getAttribute('aria-label');
  const navOk = await page.getByRole('button', { name: '画布' }).getAttribute('aria-current');
  record('A6 aria 标注 + 页签 aria-current', !!ariaOk && ariaOk.includes('无限画布') && navOk === 'page');

  await browser.close();
  preview.kill();
  console.log(`SUMMARY: ${pass}/${pass + fail} PASS`);
  process.exit(fail > 0 ? 1 : 0);
})().catch((e) => { console.error('FATAL', e); process.exit(2); });
