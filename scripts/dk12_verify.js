/**
 * DK-12 验证脚本 — 编辑器实时协同渲染（DoD 双实例探针 + 回归）
 * 前置：cd apps/desktop && npx vite build --config vite.config.editorlab.ts && npx vite build
 * DoD：A 编辑 → B 已 attach → update import → B 渲染收敛 ≤2s（applyRemote 绕行通路）
 */
const path = require('path');
const { chromium } = (() => { try { return require('playwright'); } catch { return require('/home/z/.npm-global/lib/node_modules/playwright'); } })();
const REPO = path.join(__dirname, '..');
const DIST = path.join(REPO, 'apps/desktop/dist-lab/editor-lab.html');
const results = [];
const record = (name, pass, detail) => { results.push(pass); console.log(`${pass ? 'PASS' : 'FAIL'} ${name}: ${detail}`); };

(async () => {
  const fs = require('fs');
  if (!fs.existsSync(DIST)) { console.log('FAIL 前置检查: dist-lab 缺失（先 build editorlab）'); process.exit(1); }
  const b = await chromium.launch({ args: ['--no-sandbox'] });
  const p = await b.newPage();
  p.on('pageerror', (e) => console.log('PAGE_ERR:', String(e).slice(0, 120)));
  await p.goto('file://' + DIST, { waitUntil: 'load', timeout: 30000 });
  await p.waitForFunction(() => window.__probe && window.__probe.liveCollabProbe, { timeout: 20000 });

  // DoD 主断言 ×3 连跑（确定性证明）
  for (let i = 1; i <= 3; i++) {
    const r = await p.evaluate(() => window.__probe.liveCollabProbe());
    record(`DoD-${i} A编辑→B(attach)→import→渲染收敛≤2s`, r.converged === true && r.elapsed <= 2000,
      `elapsed=${r.elapsed}ms status=${r.status} dom="${r.bDomText}"`);
  }

  // 回归：S3 语义（挂载时渲染 / 快照恢复）——复用 lab 既有探针
  const dual = await p.evaluate(() => window.__probe.dualInstanceSync());
  record('回归-A2 双实例增量同步（挂载时渲染）', dual.ok === true, `textB="${dual.textB}"`);
  const inc = await p.evaluate(() => window.__probe.incrementalProbe());
  record('回归-A3 增量渲染（事务级 DOM 更新）', inc.sameDom && inc.updated, `sameDom=${inc.sameDom} updated=${inc.updated}`);

  console.log(`\nSUMMARY: ${results.filter(Boolean).length}/${results.length} PASS`);
  await b.close();
  process.exit(results.every(Boolean) ? 0 : 1);
})().catch((e) => { console.log('FATAL:', String(e).slice(0, 200)); process.exit(2); });
