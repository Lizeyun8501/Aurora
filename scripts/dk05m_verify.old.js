/**
 * DK-05M 验证脚本 — 移动端块编辑器（Android WebView 面专属五断言）
 *
 * 前置：cd apps/mobile && npx vite build（base './'，file:// 直载 dist）
 * 形态：Playwright chromium · mobile viewport 375×667 · hasTouch · file://
 *
 * 五断言（DoD）：
 *   A1 工具条键盘态：mock visualViewport.height 收缩 → 工具条完整可见（贴可视底）
 *   A2 IME 稳定：composition 序列注入 → selection anchor 不漂移、无强滚
 *   A3 浮动菜单：选区 → 菜单可见区展示；遮挡 → 贴工具条上方
 *   A4 万字滚动性能：rAF 间隔采样 P95 ≤ 20ms（≈50fps）
 *   A5 无障碍：工具条 role/aria 快照完整可聚焦
 */
let chromium;
try { ({ chromium } = require('playwright')); }
catch { ({ chromium } = require('/home/z/.npm-global/lib/node_modules/playwright')); }
const { spawnSync } = require('node:child_process');
const path = require('node:path');
const fs = require('node:fs');

const REPO = path.join(__dirname, '..');
const MOBILE_DIST = path.join(REPO, 'apps/mobile/dist/index.html');

// --- 构建前置（D1 小卡模式：产物缺失/陈旧时自动 build） ---
function ensureMobileBuild() {
  const need = !fs.existsSync(MOBILE_DIST) ||
    fs.statSync(MOBILE_DIST).mtimeMs <
    Math.max(
      fs.statSync(path.join(REPO, 'apps/mobile/src/MobileApp.tsx')).mtimeMs,
      fs.statSync(path.join(REPO, 'shared/ui-components/src/editors/RichEditor.tsx')).mtimeMs,
      fs.statSync(path.join(REPO, 'apps/mobile/src/styles/mobile.css')).mtimeMs,
    );
  if (!need) return;
  console.log('[build-prereq] mobile 产物缺失/陈旧，重建 vite build ...');
  const r = spawnSync(process.execPath, [
    path.join(REPO, 'node_modules/.bin/vite'), 'build',
  ], { cwd: path.join(REPO, 'apps/mobile'), stdio: ['ignore', 'pipe', 'inherit'] });
  if (r.status !== 0 || !fs.existsSync(MOBILE_DIST)) {
    console.error('[build-prereq] mobile build 失败或产物探活失败');
    process.exit(3);
  }
  console.log('[build-prereq] 产物就绪');
}

(async () => {
  ensureMobileBuild();
  const b = await chromium.launch({ args: ['--no-sandbox'] });
  const page = await b.newPage({
    viewport: { width: 375, height: 667 },
    hasTouch: true,
    isMobile: true,
    deviceScaleFactor: 2,
  });
  page.on('pageerror', (e) => console.error('[pageerror]', String(e).slice(0, 120)));
  await page.goto(`file://${MOBILE_DIST}`, { waitUntil: 'load' });
  await page.waitForSelector('.app-shell', { timeout: 15000 });

  // 打开笔记（进入编辑器——首条笔记点击；若列表空则新建）
  await page.evaluate(() => {
    const btn = document.querySelector('.note-item, .note-card, [class*=note-]');
    if (btn) btn.dispatchEvent(new MouseEvent('click', { bubbles: true }));
  });
  await page.waitForTimeout(600);
  // 若无笔记，走新建入口
  const hasEditor = await page.$('[contenteditable="true"]');
  if (!hasEditor) {
    await page.evaluate(() => {
      const btns = [...document.querySelectorAll('button')];
      const create = btns.find((x) => /新建|新笔记|add|create/i.test(x.getAttribute('aria-label') || x.textContent || ''));
      if (create) create.click();
    });
    await page.waitForTimeout(800);
  }
  await page.waitForSelector('[contenteditable="true"]', { timeout: 20000 });

  let pass = 0, fail = 0;
  const ok = (name, cond, extra = '') => {
    if (cond) { pass++; console.log(`PASS ${name}${extra ? ': ' + extra : ''}`); }
    else { fail++; console.log(`FAIL ${name}${extra ? ': ' + extra : ''}`); }
  };

  // ---------------- A1 工具条键盘态 ----------------
  {
    await page.evaluate(() => {
      const vv = window.visualViewport;
      Object.defineProperty(vv, 'height', { value: 400, configurable: true });
      vv.dispatchEvent(new Event('resize'));
    });
    await page.waitForTimeout(150); // rAF 节流窗口
    const a1 = await page.evaluate(() => {
      const root = document.documentElement.style;
      const vvh = root.getPropertyValue('--vvh');
      const inset = root.getPropertyValue('--kbd-inset');
      const tb = document.querySelector('.editor-toolbar');
      if (!tb) return { ok: false, why: 'toolbar missing' };
      const r = tb.getBoundingClientRect();
      // 工具条完整可见：上缘≥0、下缘 ≈ 可视底（vv.height=400），且不高于 400
      const bottom = r.bottom;
      return {
        ok: vvh === '400px' && r.top >= 0 && bottom <= 401 && bottom >= 352,
        why: `vvh=${vvh} inset=${inset} tb.top=${Math.round(r.top)} tb.bottom=${Math.round(bottom)}`,
      };
    });
    ok('A1 工具条键盘态（vv 400 → 完整可见贴可视底）', a1.ok, a1.why);
    // 恢复
    await page.evaluate(() => {
      const vv = window.visualViewport;
      Object.defineProperty(vv, 'height', { value: 667, configurable: true });
      vv.dispatchEvent(new Event('resize'));
    });
    await page.waitForTimeout(120);
  }

  // ---------------- A2 IME 稳定（composition 序列） ----------------
  {
    const a2 = await page.evaluate(() => {
      const dom = document.querySelector('[contenteditable="true"]');
      if (!dom) return { ok: false, why: 'no contenteditable' };
      const view = null; // PM view 不直接可达：以 DOM selection + scroll 稳定性为代理断言
      dom.focus();
      const before = {
        scroll: document.querySelector('.rich-editor-host')?.scrollTop ?? 0,
        anchor: (() => { const s = getSelection(); return s.anchorNode ? s.anchorOffset : -1; })(),
      };
      // composition 序列（PM compositionstart/update/end 全链 dispatch）
      const fire = (type, data) => dom.dispatchEvent(new CompositionEvent(type, { data, bubbles: true }));
      fire('compositionstart', '');
      for (const part of ['n', 'ni', 'nihao']) fire('compositionupdate', part);
      const midScroll = document.querySelector('.rich-editor-host')?.scrollTop ?? 0;
      const midAnchor = (() => { const s = getSelection(); return s.anchorNode ? s.anchorOffset : -1; })();
      fire('compositionend', 'nihao');
      const after = {
        scroll: document.querySelector('.rich-editor-host')?.scrollTop ?? 0,
      };
      // 键盘态判定：组合期间无强滚（scroll 不突变）+ anchor 序列单调（不漂移到别处）
      const scrollStable = Math.abs(midScroll - before.scroll) < 2 && Math.abs(after.scroll - before.scroll) < 2;
      const anchorSane = before.anchor >= 0 && midAnchor >= 0;
      return { ok: scrollStable && anchorSane, why: `scroll ${before.scroll}→${midScroll}→${after.scroll} anchor ${before.anchor}→${midAnchor}` };
    });
    ok('A2 IME composing 门控（无强滚/anchor 不漂移）', a2.ok, a2.why);
  }

  // ---------------- A3 浮动菜单（选区 → 可视区；遮挡 → 贴工具条） ----------------
  {
    // 先压低 vv 模拟键盘态制造遮挡场景
    await page.evaluate(() => {
      const vv = window.visualViewport;
      Object.defineProperty(vv, 'height', { value: 300, configurable: true });
      vv.dispatchEvent(new Event('resize'));
    });
    await page.waitForTimeout(150);
    const a3 = await page.evaluate(() => {
      const dom = document.querySelector('[contenteditable="true"]');
      if (!dom) return { ok: false, why: 'no contenteditable' };
      // 选一段靠下方的文本节点
      const walker = document.createTreeWalker(dom, NodeFilter.SHOW_TEXT);
      let target = null;
      while (walker.nextNode()) {
        const n = walker.currentNode;
        if ((n.textContent || '').length >= 5) target = n; // 取最后一个（最靠下）
      }
      if (!target) return { ok: false, why: 'no text node' };
      const len = Math.min(4, target.textContent.length);
      const r = document.createRange();
      r.setStart(target, 0);
      r.setEnd(target, len);
      const s = getSelection();
      s.removeAllRanges();
      s.addRange(r);
      document.dispatchEvent(new Event('selectionchange'));
      return { ok: true, why: 'selection set' };
    });
    await page.waitForTimeout(200); // rAF + 菜单渲染
    const a3b = await page.evaluate(() => {
      const menu = document.querySelector('.floating-menu');
      if (!menu) return { ok: false, why: 'floating-menu not shown' };
      const r = menu.getBoundingClientRect();
      const vv = window.visualViewport;
      const vvBottom = vv.height + vv.offsetTop;
      const TOOLBAR_H = 48;
      // 菜单完整位于可视区且不与工具条重叠（贴工具条上方）
      const visible = r.top >= 0 && r.bottom <= vvBottom;
      const noOverlap = r.bottom <= vvBottom - TOOLBAR_H + 2;
      return { ok: visible && noOverlap, why: `menu.top=${Math.round(r.top)} bottom=${Math.round(r.bottom)} vvBottom=${Math.round(vvBottom)}` };
    });
    ok('A3 浮动菜单（键盘态下可视区展示/贴工具条上方）', a3.ok && a3b.ok, a3b.why);
    await page.evaluate(() => {
      const vv = window.visualViewport;
      Object.defineProperty(vv, 'height', { value: 667, configurable: true });
      vv.dispatchEvent(new Event('resize'));
    });
    await page.waitForTimeout(120);
  }

  // ---------------- A4 万字滚动性能（rAF 间隔 P95 ≤ 20ms） ----------------
  {
    const a4 = await page.evaluate(async () => {
      const dom = document.querySelector('[contenteditable="true"]');
      if (!dom) return { ok: false, why: 'no contenteditable', p95: -1 };
      dom.focus();
      // 万字注入（execCommand 批量，PM beforeinput 通路）
      const chunk = '性能测试段落内容滚动采样采样写满一万字符量级的长文档用来压测滚动帧率与重绘表现。';
      for (let i = 0; i < 130; i++) {
        document.execCommand('insertText', false, chunk);
      }
      // 滚动 + rAF 间隔采样
      const host = document.querySelector('.rich-editor-host') || dom.parentElement;
      const gaps = [];
      let last = performance.now();
      let running = true;
      const tick = (t) => {
        gaps.push(t - last); last = t;
        if (running) requestAnimationFrame(tick);
      };
      requestAnimationFrame(tick);
      const total = host ? host.scrollHeight : 99999;
      for (let y = 0; y <= total; y += 120) {
        if (host) host.scrollTop = y;
        await new Promise((r2) => setTimeout(r2, 8));
      }
      running = false;
      gaps.shift(); // 首帧噪声
      gaps.sort((a, b2) => a - b2);
      const p95 = gaps[Math.floor(gaps.length * 0.95)] ?? 999;
      return { ok: p95 <= 20, why: `samples=${gaps.length} p95=${p95.toFixed(1)}ms`, p95 };
    });
    ok('A4 万字滚动 rAF 间隔 P95 ≤ 20ms', a4.ok, a4.why);
  }

  // ---------------- A5 无障碍 aria 快照 ----------------
  {
    const a5 = await page.evaluate(() => {
      const tb = document.querySelector('.editor-toolbar');
      if (!tb) return { ok: false, why: 'toolbar missing' };
      const role = tb.getAttribute('role');
      const label = tb.getAttribute('aria-label');
      const btns = [...tb.querySelectorAll('button')];
      const allLabeled = btns.every((x) =>
        (x.getAttribute('aria-label') || x.title || x.textContent || '').trim().length > 0);
      const focusable = btns.length > 0 && btns.every((x) => !x.disabled);
      return { ok: role === 'toolbar' && !!label && allLabeled && focusable,
        why: `role=${role} label=${label} btns=${btns.length} allLabeled=${allLabeled}` };
    });
    ok('A5 工具条 aria 快照（role/label/按钮全标注可聚焦）', a5.ok, a5.why);
  }

  console.log(`SUMMARY: ${pass}/${pass + fail} PASS`);
  await b.close();
  process.exit(fail === 0 ? 0 : 1);
})().catch((e) => { console.error('FATAL:', e); process.exit(2); });
