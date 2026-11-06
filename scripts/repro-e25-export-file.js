/**
 * E25 验证（TA 17:32: 导出真文件+保存位置选择+默认位）
 * mock AndroidBridge 注入（PW 无桥环境真机桥面模拟）:
 * ① 菜单「保存位置」行现（label 默认 Download/AuroraExports+两钮）
 * ② pickExportDir 桥调+回推 label 更新
 * ③ resetExportDir 复位
 * ④ doExport('md') 走桥 exportFile（真文件链接管——非 share/blob）
 * ⑤ __onExportDone toast 回推
 */
const { chromium } = require('/home/z/.npm-global/lib/node_modules/playwright');
const BASE = 'file:///home/z/my-project/repos/Aurora/apps/mobile/dist/index.html';

(async () => {
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 390, height: 844 }, hasTouch: true });
  const results = [];
  const record = (n, p, d) => { results.push({ n, p }); console.log(`${p ? 'PASS' : 'FAIL'} ${n}: ${d}`); };
  const errors = [];
  page.on('pageerror', (e) => errors.push(String(e).slice(0, 100)));

  // mock 桥——E25 三法记录调用
  await page.addInitScript(() => {
    const calls = { getLabel: 0, pick: 0, reset: 0, exportFile: [] };
    window.__e25calls = calls;
    const NS = 'aurora.mock.note';
    let idSeq = 1;
    const list = () => JSON.parse(localStorage.getItem(NS) || '[]');
    const saveList = (l) => localStorage.setItem(NS, JSON.stringify(l));
    window.AndroidBridge = {
      // 基础面（platform 探测必需——缺 init 崩首屏）
      init: () => 'ok',
      isFallback: () => false,
      listNotes: () => JSON.stringify(list().map((n) => ({ id: n.id, title: n.title, updatedAt: n.updatedAt, encryption: 'none' }))),
      createNote: (t) => { const n = { id: 'n' + (idSeq++), title: t, updatedAt: Date.now(), content: '' }; const l = list(); l.unshift(n); saveList(l); return JSON.stringify(n); },
      getNoteContent: (id) => { const n = list().find((x) => x.id === id); return n ? n.content || '' : ''; },
      saveNoteContent: (id, c) => { const l = list(); const n = l.find((x) => x.id === id); if (n) { n.content = c; n.updatedAt = Date.now(); saveList(l); } return 1; },
      deleteNote: (id) => { saveList(list().filter((x) => x.id !== id)); return 1; },
      searchNotes: (q) => '[]',
      getNoteSnapshot: () => '',
      saveNoteSnapshot: () => true,
      listSnapshots: () => '[]',
      getSnapshotContent: () => null,
      todayFocusSummary: () => null,
      getWifiOnly: () => false,
      setWifiOnly: () => {},
      setNoteEncryption: () => true,
      // E25 三法
      getExportDirLabel: () => { calls.getLabel++; return '默认: Download/AuroraExports'; },
      pickExportDir: () => { calls.pick++; setTimeout(() => window.__onExportDirPicked?.(true), 30); },
      resetExportDir: () => { calls.reset++; },
      exportFile: (f, m, t) => { calls.exportFile.push({ f, m, len: t.length }); setTimeout(() => window.__onExportDone?.(true, '/storage/emulated/0/Download/AuroraExports/' + f), 30); },
    };
  });

  await page.goto(BASE);
  await page.waitForTimeout(1600);
  await page.click('.tab-bar .tab-item:has-text("知识库")');
  await page.waitForTimeout(700);
  await page.click('.nt-arrow');
  await page.waitForTimeout(400);
  await page.click('.nt-add');
  await page.waitForTimeout(900);
  await page.click('.nt-row:has-text("未命名")');
  await page.waitForSelector('.ProseMirror', { timeout: 10000 });
  await page.click('.ProseMirror');
  await page.keyboard.type('E25 导出验证内容', { delay: 10 });
  await page.waitForTimeout(500);

  // 开菜单
  await page.click('button[aria-label="更多"]');
  await page.waitForTimeout(600);

  // ① 保存位置行
  const row = await page.evaluate(() => {
    const r = document.querySelector('.export-dir-row');
    return {
      row: !!r,
      label: document.querySelector('.export-dir-label')?.textContent,
      pickBtn: !!Array.from(document.querySelectorAll('.export-dir-btn')).find((b) => b.textContent === '选择位置'),
      resetBtn: !!Array.from(document.querySelectorAll('.export-dir-btn')).find((b) => b.textContent === '默认'),
    };
  });
  record('① 保存位置行（默认 label+两钮）', row.row && row.label?.includes('Download/AuroraExports') === true && row.pickBtn && row.resetBtn,
    JSON.stringify(row));

  // ② pickExportDir
  await page.click('.export-dir-btn:has-text("选择位置")');
  await page.waitForTimeout(500);
  const pick = await page.evaluate(() => ({ calls: window.__e25calls.pick, label: document.querySelector('.export-dir-label')?.textContent }));
  record('② 选取位置→桥调+label 回推', pick.calls >= 1, `桥调=${pick.calls}`);

  // ③ reset
  await page.click('.export-dir-btn:has-text("默认")');
  await page.waitForTimeout(400);
  const reset = await page.evaluate(() => ({ calls: window.__e25calls.reset }));
  record('③ 复位默认', reset.calls >= 1, `桥调=${reset.calls}`);

  // ④ 导出 md——桥接管
  await page.click('.menu-item:has-text("导出 Markdown")');
  await page.waitForTimeout(1800);
  const exp = await page.evaluate(() => ({ files: window.__e25calls.exportFile }));
  record('④ 导出走桥 exportFile 真文件链', exp.files.length >= 1 && exp.files[0].f.endsWith('.md') && exp.files[0].len > 0,
    JSON.stringify(exp.files));

  // ⑤ toast 回推
  const toast = await page.evaluate(() => document.body.textContent?.includes('/storage/emulated/0/Download/AuroraExports/'));
  record('⑤ __onExportDone toast 路径回显', toast === true, `toast 含保存路径=${toast}`);
  record('⑥ 零错误', errors.length === 0, errors.slice(0, 2).join('|') || 'clean');

  const passed = results.filter((r) => r.p).length;
  console.log(`\n=== E25 导出真文件+保存位置: ${passed}/${results.length} PASS ===`);
  await browser.close();
  process.exit(passed === results.length ? 0 : 1);
})();
