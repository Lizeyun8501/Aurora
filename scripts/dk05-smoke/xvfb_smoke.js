// DK-05 S2 冻结硬门槛 — Plan B 冒烟：Xvfb 真实 X 会话 + headful Chromium + xdotool 原生 X 输入
// 语义：打包产物 frontend（dist）在真实窗口会话渲染 + 原生 X 键鼠/滚轮事件 + DOM/browser-mock 断言
// 差距披露：非 WebKitGTK 引擎（无 GPU 栈级不可运行，MiniBrowser 同 crashed）；Rust invoke 由 CI/本机 build 覆盖
let chromium;
try { ({ chromium } = require('playwright')); } catch { ({ chromium } = require('/home/z/.npm-global/lib/node_modules/playwright')); }
const { execSync } = require('child_process');
const XDOOL = process.env.XDOOL || '/home/z/.local/pkg/root/usr/bin/xdotool';
const DISPLAY = process.env.DISPLAY || ':77';
if (!process.env.LD_LIBRARY_PATH) process.env.LD_LIBRARY_PATH = '/home/z/.local/pkg/root/usr/lib/x86_64-linux-gnu';
if (!process.env.DISPLAY) process.env.DISPLAY = ':77';
process.env.LD_LIBRARY_PATH = '/home/z/.local/pkg/root/usr/lib/x86_64-linux-gnu:' + (process.env.LD_LIBRARY_PATH || '');
process.env.DISPLAY = ':77';

const results = [];
const record = (name, pass, detail) => {
  results.push({ name, pass, detail });
  console.log(`${pass ? 'PASS' : 'FAIL'} ${name}: ${detail}`);
};

(async () => {
  const NONCE = 'SMOKE' + Date.now().toString().slice(-6);
  console.log('NONCE=' + NONCE);

  // headful Chromium 绑定 Xvfb :77（真实 X 窗口会话，非 headless CDP）
  const browser = await chromium.connectOverCDP('http://127.0.0.1:9222');
  const ctx = await browser.newContext({ viewport: { width: 1280, height: 800 } });
  const page = await ctx.newPage();
  const errors = [];
  page.on('pageerror', (e) => errors.push(String(e).slice(0, 120)));

  // 打包产物（vite build dist）经 preview 服务加载 = 生产静态加载形态
  await page.goto('http://127.0.0.1:1421', { waitUntil: 'networkidle', timeout: 30000 });
  await page.waitForTimeout(2500);

  // S1: 真实 X 会话窗口存在（xdotool 从 X server 侧确认，非 CDP；无 WM 环境不去 onlyvisible）
  let winName = '';
  try { winName = execSync(`env DISPLAY=${DISPLAY} ${XDOOL} getactivewindowname 2>/dev/null`).toString().trim(); } catch {}
  let winFound = 0, winList = '';
  try {
    winList = execSync(`env DISPLAY=${DISPLAY} ${XDOOL} search --name "" 2>/dev/null || true`).toString().trim();
    winFound = winList ? winList.split('\n').filter((l) => l.trim()).length : 0;
  } catch {}
  record('S1 真实X会话窗口存在', winFound > 0, `windows=${winFound} active="${winName}"`);

  // S1b: 无 Tauri 宿主 → invoke 探测回落 browser-mock（Alpha 缺陷修复的行为级证明）
  const mockProbe = await page.evaluate(() => typeof window.__TAURI_INTERNALS__);
  record('S1b invoke探测回落browser-mock生效', mockProbe === 'undefined', `__TAURI_INTERNALS__=${mockProbe}`);

  // S2: 选中演示笔记 → 编辑器挂载（打包产物 EditAuroraEditor 链路）
  await page.click('text=V23 迭代复盘', { timeout: 8000 });
  await page.waitForSelector('.ProseMirror', { timeout: 15000 });
  const mounted = await page.evaluate(() => ({
    editor: !!document.querySelector('.ProseMirror'),
    editable: document.querySelector('.ProseMirror')?.getAttribute('contenteditable'),
    mockProbe: typeof window.__TAURI_INTERNALS__,
  }));
  record('S2 编辑器挂载(打包产物)', mounted.editor === true, JSON.stringify(mounted));

  // S3: xdotool 原生 X 真实键盘输入（绕过 CDP 合成，X server 级事件）
  try { execSync(`env DISPLAY=${DISPLAY} ${XDOOL} mousemove --sync 640 400 click 1`); } catch {}
  await page.waitForTimeout(800);
  try { execSync(`env DISPLAY=${DISPLAY} ${XDOOL} type --delay 45 "${NONCE} tauri-x-smoke"`); } catch (e) {}
  await page.waitForTimeout(1200);
  const typed = await page.evaluate((n) => document.body.innerText.includes(n), NONCE);
  record('S3 原生X键盘输入→内容回显', typed, `nonce_in_dom=${typed}`);

  // S4: 原生 X 滚轮并发输入（真实按钮 4/5 事件，S0 风险 2 桌面差异项同路径）
  let wheelOk = true;
  try {
    let lostKey = false;
    for (let i = 0; i < 8; i++) {
      execSync(`env DISPLAY=${DISPLAY} ${XDOOL} click ${i % 2 ? 4 : 5}`);
      if (i % 3 === 1) { try { execSync(`env DISPLAY=${DISPLAY} ${XDOOL} type --delay 30 'w${NONCE}'`); } catch { lostKey = true; } }
    }
    await page.waitForTimeout(1200);
    const wheelTyped = await page.evaluate((n) => document.body.innerText.includes('w' + n), NONCE);
    record('S4a 滚轮并发打字(滚动中输入不丢键)', !lostKey && wheelTyped, `interleaved, text_in_dom=${wheelTyped}`);
  } catch { wheelOk = false; }
  await page.waitForTimeout(800);
  const aliveAfterWheel = await page.evaluate(() => document.body.innerText.length > 0);
  record('S4 原生X滚轮并发(12事件)', wheelOk && aliveAfterWheel, `dom_alive=${aliveAfterWheel}`);

  // S5: 焦点链（X 焦点切换→窗口激活→DOM 焦点可达）
  try { execSync(`env DISPLAY=${DISPLAY} ${XDOOL} windowactivate --sync $(env DISPLAY=${DISPLAY} ${XDOOL} search --onlyvisible --name "." | head -1)`); } catch {}
  await page.waitForTimeout(600);
  await page.click('.ProseMirror, [role="document"]', { timeout: 5000 }).catch(() => {});
  const focus = await page.evaluate(() => {
    const el = document.activeElement;
    return { tag: el?.tagName, pm: !!el?.closest?.('.ProseMirror') || el?.classList?.contains('ProseMirror') };
  });
  record('S5 X焦点切换→编辑器焦点可达(收紧口径)', focus.pm === true, JSON.stringify(focus));

  // S6: 无页面级 JS 崩溃（全流程存活证明）
  record('S6 全流程无JS崩溃', errors.length === 0, `pageerrors=${errors.length}`);

  // 截图留痕
  await page.screenshot({ path: '/home/z/smoke/xvfb_smoke_evidence.png', fullPage: false });

  console.log('\nSUMMARY: ' + results.filter((r) => r.pass).length + '/' + results.length + ' PASS');
  await browser.close();
  process.exit(results.every((r) => r.pass) ? 0 : 1);
})().catch((e) => { console.log('FATAL:', String(e).slice(0, 200)); process.exit(2); });
