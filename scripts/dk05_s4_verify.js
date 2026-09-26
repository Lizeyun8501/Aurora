/**
 * DK-05 S4 收尾验证 — Playwright(Chromium) 驱动
 *
 * A 段（editor-lab file://）: 性能采样——快照恢复首开耗时（热身态）+
 *   大文档灌入耗时。
 * B 段（生产 build 产物 preview, browser-mock）: S4-1 损坏快照降级
 *   （非法 base64 → content 文本兜底，编辑器不白屏）+ 首开链路性能
 *   （切笔记 waitForSelector 耗时采样）+ S1-S3 核心链路复验（回归面）。
 */
let chromium;
try { ({ chromium } = require('playwright')); }
catch { ({ chromium } = require('/home/z/.npm-global/lib/node_modules/playwright')); }
const { spawn } = require('node:child_process');
const path = require('node:path');
const fs = require('node:fs');

const REPO = path.join(__dirname, '..');
const LAB = `file://${path.join(REPO, 'apps/desktop/dist-lab/editor-lab.html')}`;
const DIST_INDEX = path.join(REPO, 'apps/desktop/dist/index.html');
if (!fs.existsSync(DIST_INDEX)) {
  console.error('dist/index.html 不存在 — 先执行: cd apps/desktop && npx vite build');
  process.exit(2);
}

const results = [];
const record = (name, pass, detail) => {
  results.push({ name, pass });
  console.log(`${pass ? 'PASS' : 'FAIL'} ${name}: ${detail}`);
};

(async () => {
  const browser = await chromium.launch({ args: ['--no-proxy-server'] });
  const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });

  // ── A 段: 性能采样 ──
  await page.goto(LAB);
  await page.waitForFunction(() => window.__probe && window.__pmState, null, { timeout: 20000 });

  const restore = await page.evaluate(() => window.__probe.restorePerf());
  record('A1 快照恢复首开（热身态）', restore.ms < 500 && restore.textLen > 0, `${restore.ms}ms len=${restore.textLen}`);

  const big = await page.evaluate(() => window.__probe.bigDocPerf(300));
  record('A2 大文档灌入 300 段', big.ms < 3000 && big.count >= 300, `${big.ms}ms nodes=${big.count}`);

  // ── B 段: 生产降级路径 + 首开性能 + 核心回归 ──
  // 直接 node + vite bin（绕过 npx 冷启动抖动）+ 探活重试
  const VITE_BIN = path.join(REPO, 'node_modules/.bin/vite');
  const dev = spawn(process.execPath, [VITE_BIN, 'preview', '--port', '1421', '--strictPort', '--host', '127.0.0.1'], {
    cwd: path.join(REPO, 'apps/desktop'),
    stdio: ['ignore', 'pipe', 'pipe'],
    detached: true,
  });
  dev.stderr.on('data', (d) => { const t = String(d).trim(); if (t) console.error('[preview]', t.slice(0, 160)); });
  let up = false;
  for (let i = 0; i < 30 && !up; i++) {
    await new Promise((r) => setTimeout(r, 1000));
    up = await new Promise((res) => {
      require('node:http').get('http://127.0.0.1:1421/', (r2) => { r2.resume(); res(r2.statusCode === 200); }).on('error', () => res(false));
    });
  }
  console.log(`[preview] 探活: ${up ? 'up' : 'FAIL(30s)'}`);
  try {
    await page.goto('http://127.0.0.1:1421', { waitUntil: 'networkidle', timeout: 60000 });
    // 热身：先打开一次笔记（动态 import wasm 链路就位）
    await page.click('text=NTN HARQ 反馈禁用场景', { timeout: 15000 });
    await page.waitForSelector('[role="textbox"] .ProseMirror', { timeout: 20000 });

    // B1 损坏快照降级（S4-1）：非法 base64 快照 → content 文本兜底，不白屏
    await page.evaluate(() => {
      (window.__snapshots = window.__snapshots || {})['n2'] = '!!!not-valid-loro-snapshot!!!';
    });
    await page.click('text=V23 迭代复盘', { timeout: 15000 });
    await page.waitForSelector('[role="textbox"] .ProseMirror', { timeout: 20000 });
    await page.waitForTimeout(500);
    await page.click('text=NTN HARQ 反馈禁用场景', { timeout: 15000 });
    await page.waitForSelector('[role="textbox"] .ProseMirror', { timeout: 20000 });
    await page.waitForTimeout(800);
    const fallback = await page.evaluate(() => {
      const pm = document.querySelector('[role="textbox"] .ProseMirror');
      return {
        alive: !!pm,
        hasContent: pm.textContent.includes('跨层调度专利分析要点'),
        warnCaptured: window.__fallbackWarn === true,
      };
    });
    record('B1 损坏快照降级（content 文本兜底不白屏）', fallback.alive && fallback.hasContent,
      `alive=${fallback.alive} content=${fallback.hasContent}`);

    // B2 首开链路性能（切笔记 waitForSelector，3 次采样取中位）
    const samples = [];
    for (let i = 0; i < 3; i++) {
      const t0 = Date.now();
      await page.click(i % 2 === 0 ? 'text=V23 迭代复盘' : 'text=NTN HARQ 反馈禁用场景', { timeout: 15000 });
      await page.waitForSelector('[role="textbox"] .ProseMirror', { timeout: 10000 });
      await page.waitForTimeout(300);
      samples.push(Date.now() - t0);
    }
    samples.sort((a, b) => a - b);
    const median = samples[1];
    record('B2 笔记切换首开（中位）', median < 3000, `median=${median}ms samples=${samples.join('/')}`);

    // B3 核心回归：编辑 → 双写 → 切回等价（S2/S3 主链路复验）
    await page.click('text=V23 迭代复盘', { timeout: 15000 });
    await page.waitForSelector('[role="textbox"] .ProseMirror', { timeout: 20000 });
    await page.evaluate(() => {
      const pm = document.querySelector('[role="textbox"] .ProseMirror');
      pm.focus();
      const sel = window.getSelection();
      const range = document.createRange();
      range.selectNodeContents(pm);
      range.collapse(false);
      sel.removeAllRanges();
      sel.addRange(range);
    });
    await page.keyboard.type('S4收尾回归');
    await page.waitForTimeout(1800);
    await page.click('text=Rust 异步锁安全清单', { timeout: 15000 });
    await page.waitForTimeout(300);
    await page.click('text=V23 迭代复盘', { timeout: 15000 });
    await page.waitForSelector('[role="textbox"] .ProseMirror', { timeout: 20000 });
    await page.waitForTimeout(600);
    const regr = await page.evaluate(() => {
      const pm = document.querySelector('[role="textbox"] .ProseMirror');
      return {
        snap: !!window.__snapshots['n1'],
        text: pm.textContent.includes('S4收尾回归'),
      };
    });
    record('B3 核心回归（编辑→双写→快照恢复等价）', regr.snap && regr.text, `snap=${regr.snap} text=${regr.text}`);
  } finally {
    try { process.kill(-dev.pid); } catch { /* already gone */ }
  }

  browser.close();
  const failed = results.filter((r) => !r.pass).length;
  console.log(`\nSUMMARY: ${results.length - failed}/${results.length} PASS${failed ? ` — ${failed} FAIL` : ''}`);
  process.exit(failed ? 1 : 0);
})();
