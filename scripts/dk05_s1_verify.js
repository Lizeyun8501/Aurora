/**
 * DK-05 S1 只读接入验证 — Playwright(Chromium) 驱动
 *
 * A 段（editor-lab file://）: schema 全节点 fixture 块级渲染
 *   （对照 schema 合并对照表全类型）+ 只读锁定 + Loro 同步。
 * B 段（生产 vite dev, browser-mock 模式）: EditorPane 只读挂载 +
 *   R-04 A 项正文区 a11y 探针（role/aria-label/tabIndex/焦点环）。
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

// Bravo 复核基建缺陷②：脚本自包含预检——dist 陈旧/缺失时明确提示
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
  // --no-proxy-server: 本环境 Chromium 层有代理配置，localhost 直连会被劫持
  const browser = await chromium.launch({ args: ['--no-proxy-server'] });
  const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });

  // ── A 段: editor-lab 全节点 fixture ──
  await page.goto(LAB);
  await page.waitForFunction(() => window.__probe && window.__pmState, null, { timeout: 20000 });

  const childCount = await page.evaluate(() => window.__probe.loadFullSchemaDoc());
  record('A1 全节点 fixture 加载（15 块类型）', childCount === 15, `childCount=${childCount}`);

  const editable = await page.evaluate(() => window.__probe.setEditable(false));
  record('A2 只读锁定生效', editable === false, `view.editable=${editable}`);

  // 块级 DOM 渲染（对照 schema 合并对照表 — 实战语义版）
  const dom = await page.evaluate(() => {
    const $ = (sel) => document.querySelectorAll(sel).length;
    return {
      h1: $('h1'), h2: $('h2'), h3: $('h3'),
      strong: $('strong'), em: $('em'), u: $('u'),
      codeBlock: $('pre code'),
      blockquote: $('blockquote'),
      ul: $('ul'), ol: $('ol'), li: $('li'),
      taskBlock: $('div.task-block'),
      taskChecked: $('div.task-block[data-checked="true"]'),
      table: $('table'), tr: $('tr'), td: $('td'),
      hr: $('hr'),
      embed: $('div.embed-block'),
      aiSuggestion: $('div.ai-suggestion'),
    };
  });
  const expected = { h1: 1, h2: 1, h3: 1, strong: 1, em: 1, u: 1, codeBlock: 1, blockquote: 1, ul: 1, ol: 1, li: 4, taskBlock: 2, taskChecked: 1, table: 1, tr: 2, td: 4, hr: 1, embed: 1, aiSuggestion: 1 };
  const bad = Object.entries(expected).filter(([k, v]) => dom[k] !== v);
  record('A3 块级渲染对照表（19 类全对）', bad.length === 0,
    bad.length ? `偏差: ${bad.map(([k, v]) => `${k}=${dom[k]}≠${v}`).join(', ')}` : '19 类计数全中');

  const loroOk = await page.evaluate(() => {
    const snap = window.__loro.export({ mode: 'snapshot' });
    return snap && snap.length > 200; // 15 块 + 文本 — 同步进 LoroDoc
  });
  record('A4 fixture 同步进 LoroDoc', loroOk, 'snapshot 导出非空');

  // ── B 段: 生产 browser-mock（EditorPane 只读挂载 + a11y A 项） ──
  // 用 build 产物 preview（= Tauri 生产静态加载形态；wasm data URL 内联在
  // build 侧已配好。dev 模式需 vite-plugin-wasm 才能跑 loro——未引入）
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
    // 点选第一条演示笔记（V23 迭代复盘）
    await page.click('text=V23 迭代复盘', { timeout: 15000 });
    await page.waitForSelector('[role="textbox"] .ProseMirror', { timeout: 20000 }); // S2 起正文区 role=textbox

    const a11y = await page.evaluate(() => {
      const el = document.querySelector('[role="textbox"]');
      const pm = el?.querySelector('.ProseMirror');
      return {
        role: el?.getAttribute('role'),
        label: el?.getAttribute('aria-label'),
        tabIndex: el?.getAttribute('tabIndex'),
        contentEditable: pm?.getAttribute('contenteditable'),
        paras: pm?.querySelectorAll('p').length ?? 0,
        text: pm?.textContent ?? '',
      };
    });
    record('B1 role=textbox + aria-label', a11y.role === 'textbox' && !!a11y.label, `label="${a11y.label}"`); // S2 起正文区 role=textbox
    record('B2 焦点可达（tabIndex=0）', a11y.tabIndex === '0', `tabIndex=${a11y.tabIndex}`);
    record('B3 编辑区挂载（contenteditable 就绪）', a11y.contentEditable === 'true', `ce=${a11y.contentEditable}`); // S2 起可编辑态
    record('B4 演示笔记段落渲染', a11y.paras >= 5 && a11y.text.includes('I0 安全收口'), `paras=${a11y.paras}`);

    // 焦点环可见（R-04: focus 态 outline）
    const ring = await page.evaluate(() => {
      const el = document.querySelector('[role="textbox"]');
      el.focus();
      return new Promise((res) => setTimeout(() => {
        const s = getComputedStyle(el);
        res({ width: s.outlineWidth, color: s.outlineColor, focused: document.activeElement === el });
      }, 120));
    });
    record('B5 焦点环可见（focus 态 outline 2px）', ring.focused && ring.width === '2px', `outline=${ring.width} ${ring.color}`);
  } finally {
    try { process.kill(-dev.pid); } catch { /* already gone */ }
  }

  browser.close();
  const failed = results.filter((r) => !r.pass).length;
  console.log(`\nSUMMARY: ${results.length - failed}/${results.length} PASS${failed ? ` — ${failed} FAIL` : ''}`);
  process.exit(failed ? 1 : 0);
})();
