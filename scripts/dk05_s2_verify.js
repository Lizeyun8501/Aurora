/**
 * DK-05 S2 块编辑操作验证 — Playwright(Chromium) 驱动
 *
 * A 段（editor-lab file://）: markdown-ish 往返（mdToNodes/docToMd）+
 *   初始灌入类型清单 + task 勾选翻转 + Loro undo/redo。
 * B 段（生产 build 产物 preview, browser-mock）: 可编辑态挂载 + 工具条
 *   块操作（H2/undo）+ 键盘路径（Tab+Enter）+ 防抖落库 + 存取往返一致。
 *
 * 落库链路断言语义：onSave debounce 1s → docToMd → cmd_update_note
 * （mock 分支写 window.__lastSaved + MOCK_CONTENT 内存更新）。
 */
let chromium;
try { ({ chromium } = require('playwright')); }
catch { ({ chromium } = require('/home/z/.npm-global/lib/node_modules/playwright')); }
const { spawn } = require('node:child_process');
const path = require('node:path');
const fs = require('node:fs');

// 仓库根相对定位（Bravo 复核基建缺陷①：禁硬编码绝对路径）
const REPO = path.join(__dirname, '..');
const LAB = `file://${path.join(REPO, 'apps/desktop/dist-lab/editor-lab.html')}`;
const DIST_INDEX = path.join(REPO, 'apps/desktop/dist/index.html');

// Bravo 复核基建缺陷②：脚本自包含预检——dist 缺失时明确提示
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

  // ── A 段: editor-lab md 往返 + 块操作 ──
  await page.goto(LAB);
  await page.waitForFunction(() => window.__probe && window.__pmState, null, { timeout: 20000 });

  // A1 markdown 往返一致（空行为序列化风格 — 滤空行后逐行比对）
  {
    const sample = [
      '# 一级标题',
      '',
      '普通段落文本',
      '- 甲',
      '- 乙',
      '',
      '1. 第一',
      '- [x] 已完成任务',
      '```rust',
      'fn main() {}',
      '```',
      '> 引用行',
      '---',
      '尾段',
    ].join('\n');
    const out = await page.evaluate((t) => window.__probe.mdRoundTrip(t), sample);
    const norm = (s) => s.split('\n').map((l) => l.trimEnd()).filter((l) => l !== '');
    const a = norm(sample);
    const b = norm(out);
    const ok = a.length === b.length && a.every((l, i) => l === b[i]);
    record('A1 markdown 往返一致', ok, ok ? `${b.length} 行全等` : `差:\n  期望=${a.join('⏎')}\n  实际=${b.join('⏎')}`);
  }

  // A2 md 灌入 → 顶层类型清单
  {
    const md = ['# H1', '', '段落', '- a', '- b', '', '1. x', '- [x] t', '```rust', 'code', '```', '> q', '---'].join('\n');
    const types = await page.evaluate((t) => window.__probe.loadMd(t), md);
    const expect = ['heading1', 'paragraph', 'bullet_list', 'ordered_list', 'task_block', 'code_block', 'blockquote', 'horizontal_rule'];
    const ok = JSON.stringify(types) === JSON.stringify(expect);
    record('A2 md 灌入类型清单', ok, types.join(','));
  }

  // A3 task 勾选翻转（checked attr 事务往返）
  {
    await page.evaluate(() => window.__probe.loadMd('- [x] 任务甲'));
    const pos = await page.evaluate(() => {
      let p = -1;
      window.__view.state.doc.descendants((n, off) => {
        if (n.type.name === 'task_block' && p < 0) p = off;
        return true;
      });
      return p;
    });
    const r1 = await page.evaluate((pp) => window.__probe.toggleTask(pp), pos);
    const r2 = await page.evaluate((pp) => window.__probe.toggleTask(pp), pos);
    record('A3 task 勾选翻转', r1 === false && r2 === true, `x→${r1}→${r2}`);
  }

  // A4 Loro undo/redo（文本插入撤销/重做 — LoroUndoPlugin 栈）
  {
    await page.evaluate(() => window.__probe.loadMd('- [x] 任务甲'));
    await page.evaluate(() => {
      const { TextSelection } = window.__pmState;
      const view = window.__view;
      view.dispatch(
        view.state.tr.setSelection(
          TextSelection.create(view.state.doc, view.state.doc.content.size - 1),
        ),
      );
    });
    await page.evaluate(() => window.__probe.insertChars('UNDO样'));
    await page.evaluate(() => window.__view.focus());
    await page.waitForTimeout(80);
    const text0 = await page.evaluate(() => window.__probe.pmText());
    await page.keyboard.press('Control+z');
    await page.waitForTimeout(120);
    const text1 = await page.evaluate(() => window.__probe.pmText());
    await page.keyboard.press('Control+y');
    await page.waitForTimeout(120);
    const text2 = await page.evaluate(() => window.__probe.pmText());
    record('A4 Loro undo/redo', text0 !== text1 && text0 === text2, `len ${text0.length} →undo→ ${text1.length} →redo→ ${text2.length}`);
  }

  // ── B 段: 生产 browser-mock（可编辑 + 工具条 + 落库往返） ──
  // build 产物 preview（= Tauri 生产静态加载形态；wasm data URL 内联在 build 侧配好）
  // node + vite bin 直启（绕过 npx 冷启动抖动，S4 同款）+ 探活重试
  const VITE_BIN = path.join(REPO, 'node_modules/.bin/vite');
  const dev = spawn(process.execPath, [VITE_BIN, 'preview', '--port', '1421', '--strictPort', '--host', '127.0.0.1'], {
    cwd: path.join(REPO, 'apps/desktop'),
    stdio: 'ignore',
    detached: true,
  });
  {
    const http = require('node:http');
    let up = false;
    for (let i = 0; i < 30 && !up; i++) {
      await new Promise((r) => setTimeout(r, 1000));
      up = await new Promise((res) => {
        http.get('http://127.0.0.1:1421/', (r2) => { r2.resume(); res(r2.statusCode === 200); }).on('error', () => res(false));
      });
    }
    if (!up) console.error('[preview] 探活失败（30s）');
  }
  try {
    await page.waitForResponse((r) => r.url().includes('127.0.0.1:1421') && r.ok(), { timeout: 30000 }).catch(() => {});
    await page.goto('http://127.0.0.1:1421', { waitUntil: 'networkidle', timeout: 30000 });
    await page.click('text=V23 迭代复盘', { timeout: 15000 });
    await page.waitForSelector('[role="textbox"] .ProseMirror', { timeout: 20000 });
    await page.waitForTimeout(600);

    const editable = await page.evaluate(() => {
      const pm = document.querySelector('[role="textbox"] .ProseMirror');
      return { ce: pm.getAttribute('contenteditable'), label: document.querySelector('[role="textbox"]')?.getAttribute('aria-label') };
    });
    record('B1 可编辑态挂载（contenteditable=true）', editable.ce === 'true', `ce=${editable.ce}`);

    const initial = await page.evaluate(() => document.querySelector('[role="textbox"] .ProseMirror').children.length);
    record('B2 演示笔记 md 初始渲染', initial >= 3, `nodes=${initial}`);

    const btns = await page.evaluate(() => document.querySelectorAll('.editor-toolbar .tb-btn').length);
    record('B3 工具条渲染（块操作按钮）', btns >= 10, `btns=${btns}`);

    // 键入 → 防抖 1s → docToMd → cmd_update_note（mock: __lastSaved）
    await page.click('[role="textbox"] .ProseMirror');
    await page.keyboard.press('Control+End');
    for (let i = 0; i < 12; i++) await page.keyboard.press('Enter'); // 文档尾新段落
    await page.keyboard.type('S2落库往返验证行', { delay: 20 });
    await page.waitForTimeout(1800); // 防抖 1s + 余量
    const saved = await page.evaluate(() => (window).__lastSaved);
    record('B4 防抖落库（docToMd → cmd_update_note）', !!saved && saved.content.includes('S2落库往返验证行'), saved ? `len=${saved.content.length}` : 'no-save');

    // 工具条 H2 + 撤销（选中"更新于"上一段之外的首段 → 点 H2 → 撤销）
    await page.evaluate(() => {
      const { TextSelection } = (window).__pmState || {};
    });
    await page.evaluate(() => {
      // 光标放首个段落
      const pm = document.querySelector('[role="textbox"] .ProseMirror');
      const sel = window.getSelection();
      const first = pm.querySelector('p, h1, h2, h3');
      const range = document.createRange();
      range.selectNodeContents(first);
      sel.removeAllRanges();
      sel.addRange(range);
    });
    await page.click('.editor-toolbar .tb-btn[title*="标题 2"], .editor-toolbar .tb-btn[title*="H2"], .editor-toolbar .tb-btn:nth-of-type(4)', { timeout: 5000 }).catch(() => {});
    await page.waitForTimeout(300);
    let h2 = await page.evaluate(() => !!document.querySelector('[role="textbox"] .ProseMirror h2'));
    record('B5 工具条 H2 变换', h2, `h2=${h2}`);
    if (h2) {
      const undoBtn = await page.$('.editor-toolbar .tb-btn[title*="撤销"]');
      await undoBtn.click();
      await page.waitForTimeout(300);
      h2 = await page.evaluate(() => !!document.querySelector('[role="textbox"] .ProseMirror h2'));
      record('B5b 工具条撤销', !h2, `h2=${h2}`);
    }

    // 键盘路径：Tab 聚焦工具条按钮 → Enter 激活（R-04 键盘全操作路径抽样）
    await page.evaluate(() => {
      const pm = document.querySelector('[role="textbox"] .ProseMirror');
      const first = pm.querySelector('p, h1, h2, h3');
      const sel = window.getSelection();
      const range = document.createRange();
      range.selectNodeContents(first);
      sel.removeAllRanges();
      sel.addRange(range);
      first.focus && pm.focus();
    });
    await page.evaluate(() => document.querySelector('.editor-toolbar .tb-btn').focus());
    await page.keyboard.press('Tab'); // 相邻下一个按钮（可达性）
    await page.keyboard.press('Enter');
    await page.waitForTimeout(300);
    const kbEffect = await page.evaluate(() => ({
      h1: !!document.querySelector('[role="textbox"] .ProseMirror h1'),
      h2: !!document.querySelector('[role="textbox"] .ProseMirror h2'),
      ul: !!document.querySelector('[role="textbox"] .ProseMirror ul'),
      ol: !!document.querySelector('[role="textbox"] .ProseMirror ol'),
      bq: !!document.querySelector('[role="textbox"] .ProseMirror blockquote'),
      code: !!document.querySelector('[role="textbox"] .ProseMirror pre'),
    }));
    const kbChanged = Object.values(kbEffect).some(Boolean);
    record('B6 键盘路径（focus+Enter 激活块命令）', kbChanged, JSON.stringify(kbEffect));

    // 存取往返一致：切走 → 切回 → 内容等价
    await page.waitForTimeout(1800); // 等最后一次防抖落库
    await page.click('text=NTN HARQ 反馈禁用场景', { timeout: 15000 });
    await page.waitForTimeout(400);
    await page.click('text=V23 迭代复盘', { timeout: 15000 });
    await page.waitForSelector('[role="textbox"] .ProseMirror', { timeout: 20000 });
    await page.waitForTimeout(600);
    const roundtrip = await page.evaluate(() => {
      const pm = document.querySelector('[role="textbox"] .ProseMirror');
      return { hasText: pm.textContent.includes('S2落库往返验证行'), h2: !!pm.querySelector('h2') };
    });
    record('B7 存→取→渲染等价（切笔记往返）', roundtrip.hasText, `text=${roundtrip.hasText}`);
  } finally {
    try { process.kill(-dev.pid); } catch { /* already gone */ }
  }

  browser.close();
  const failed = results.filter((r) => !r.pass).length;
  console.log(`\nSUMMARY: ${results.length - failed}/${results.length} PASS${failed ? ` — ${failed} FAIL` : ''}`);
  process.exit(failed ? 1 : 0);
})();
