/**
 * DK-05 S3 Loro CRDT 绑定（双向）验证 — Playwright(Chromium) 驱动
 *
 * A 段（editor-lab file://）: 快照往返（生产同款装配恢复）+
 *   双实例增量同步（export update → import 收敛）+ 增量渲染
 *   （事务级 DOM 更新，非全量重挂）。
 * B 段（生产 build 产物 preview, browser-mock）: 快照主链路双写
 *   （saveNoteSnapshot + content 双写）+ 快照恢复路径证明
 *   （清空 content 后重开，快照独立生效）+ undo 加载基线。
 *
 * 快照通路：onSave debounce 1s → doc.export snapshot → bytesToBase64 →
 * cmd_save_note_snapshot（CRDT 合并语义，mobile save_note_snapshot_impl 同款）。
 */
let chromium;
try { ({ chromium } = require('playwright')); }
catch { ({ chromium } = require('/home/z/.npm-global/lib/node_modules/playwright')); }
const { spawn } = require('node:child_process');
const path = require('node:path');
const fs = require('node:fs');

// 仓库根相对定位 + dist 预检（Bravo 基建门槛）
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
  // --no-proxy-server: 本环境 Chromium 代理层会劫持 localhost
  const browser = await chromium.launch({ args: ['--no-proxy-server'] });
  const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });

  // ── A 段: lab CRDT 三面 ──
  await page.goto(LAB);
  await page.waitForFunction(() => window.__probe && window.__pmState, null, { timeout: 20000 });

  await page.evaluate(() => window.__probe.loadMd('# 往返基准\n\n快照往返内容行'));
  const rt = await page.evaluate(() => window.__probe.snapshotRoundTrip());
  record('A1 快照往返（生产同款装配恢复）', rt.equal, `len ${rt.len1}==${rt.len2}`);

  const sync = await page.evaluate(() => window.__probe.dualInstanceSync());
  record('A2 双实例增量同步（update→import 收敛）', sync.ok, `textB="${sync.textB}" status=${sync.status} updatesLen=${sync.updatesLen} loroText=${sync.loroText}`);

  const inc = await page.evaluate(() => window.__probe.incrementalProbe());
  record('A3 增量渲染（事务级 DOM 更新）', inc.sameDom && inc.updated, `sameDom=${inc.sameDom} updated=${inc.updated}`);

  // ── B 段: 生产快照主链路 ──
  const dev = spawn('npx', ['vite', 'preview', '--port', '1421', '--strictPort', '--host', '127.0.0.1'], {
    cwd: path.join(REPO, 'apps/desktop'),
    stdio: 'ignore',
    detached: true,
  });
  try {
    await page.waitForResponse((r) => r.url().includes('127.0.0.1:1421') && r.ok(), { timeout: 30000 }).catch(() => {});
    await page.goto('http://127.0.0.1:1421', { waitUntil: 'networkidle', timeout: 30000 });
    await page.click('text=V23 迭代复盘', { timeout: 15000 });
    await page.waitForSelector('[role="textbox"] .ProseMirror', { timeout: 20000 });

    // B1 编辑 → 防抖双写（快照 + content）
    await page.click('[role="textbox"] .ProseMirror');
    await page.keyboard.press('Control+End');
    await page.keyboard.type('S3快照主链路行');
    await page.waitForTimeout(2000);
    const dual = await page.evaluate(() => {
      const snap = (window).__snapshots && (window).__snapshots.n1;
      const saved = (window).__lastSaved;
      return {
        hasSnap: !!snap && snap.length > 100,
        contentWritten: !!saved && saved.content.includes('S3快照主链路行'),
      };
    });
    record('B1 防抖双写（快照 + content）', dual.hasSnap && dual.contentWritten,
      `snap=${dual.hasSnap} content=${dual.contentWritten}`);

    // B2 快照恢复路径证明：清空 content 双写值（数据层）→ 切走切回 →
    // 快照独立生效（渲染等价与 content 无关）
    await page.evaluate(() => {
      if ((window).__lastSaved) (window).__lastSaved.content = 'CLEARED';
    });
    await page.click('text=NTN HARQ 反馈禁用场景', { timeout: 15000 });
    await page.waitForTimeout(400);
    await page.click('text=V23 迭代复盘', { timeout: 15000 });
    await page.waitForSelector('[role="textbox"] .ProseMirror', { timeout: 20000 });
    await page.waitForTimeout(700);
    const restore = await page.evaluate(() => {
      const pm = document.querySelector('[role="textbox"] .ProseMirror');
      return {
        hasEdit: pm.textContent.includes('S3快照主链路行'),
        hasBase: pm.textContent.includes('I0 安全收口'),
      };
    });
    record('B2 快照恢复路径（content 清空仍等价）', restore.hasEdit && restore.hasBase,
      `edit=${restore.hasEdit} base=${restore.hasBase}`);

    // B3 undo 加载基线：快照恢复点之下不可撤（恢复后 undo 不清空文档）
    await page.evaluate(() => {
      const view = document.querySelector('[role="textbox"] .ProseMirror');
      view.focus();
    });
    await page.keyboard.press('Control+End');
    await page.keyboard.type('UNDO基线');
    await page.waitForTimeout(200);
    for (let i = 0; i < 12; i++) {
      await page.keyboard.press('Control+z');
      await page.waitForTimeout(40);
    }
    const undoFloor = await page.evaluate(() => {
      const pm = document.querySelector('[role="textbox"] .ProseMirror');
      return pm.textContent.length;
    });
    record('B3 undo 加载基线（连续撤销不清空文档）', undoFloor > 50, `len=${undoFloor}`);
  } finally {
    try { process.kill(-dev.pid); } catch { /* already gone */ }
  }

  browser.close();
  const failed = results.filter((r) => !r.pass).length;
  console.log(`\nSUMMARY: ${results.length - failed}/${results.length} PASS${failed ? ` — ${failed} FAIL` : ''}`);
  process.exit(failed ? 1 : 0);
})();
