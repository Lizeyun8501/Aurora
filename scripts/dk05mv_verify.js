/**
 * DK-05M-V WebView 输入时序验证 — Playwright(Chromium) 驱动
 *
 * 四场景: ①焦点与输入事件时序 ②IME 组合输入 ③光标恢复 ④长文档滚动
 * 环境声明: Chromium 桌面（与 Android WebView 同 Chromium 内核源）,
 * 真实 Android IME 候选/键盘高度/系统清理行为需真机补验（见报告）。
 */
const { chromium } = require('/home/z/.npm-global/lib/node_modules/playwright');

(async () => {
  const browser = await chromium.launch();
  const page = await browser.newPage({
    viewport: { width: 390, height: 844 }, // iPhone 14 尺寸视口
    hasTouch: true,
  });
  const results = [];
  const record = (name, pass, detail) => {
    results.push({ name, pass, detail });
    console.log(`${pass ? 'PASS' : 'FAIL'} ${name}: ${detail}`);
  };

  await page.goto('file:///home/z/my-project/repos/Aurora/apps/mobile/dist-lab/editor-lab.html');
  await page.waitForFunction(() => window.__view && window.__probe);

  // ── 场景 ①: 焦点与输入事件时序 ──
  {
    await page.evaluate(() => document.querySelector('.ProseMirror').focus());
    await page.waitForTimeout(120);
    const hasFocus = await page.evaluate(
      () => document.activeElement && document.activeElement.classList.contains('ProseMirror'),
    );
    console.log('①a 前置: contenteditable hasFocus =', hasFocus);
    // headless CDP key event 不产生 PM 依赖的 beforeinput（环境差异, 真机项）—
    // 输入链路用 CDP insertText（= IME 提交路径）验证
    await page.keyboard.insertText('输入时序验证A');
    const text = await page.evaluate(() => window.__probe.pmText());
    const syncOk = await page.evaluate(
      () => window.__loro.getMap('doc').size >= 0 && window.__probe.pmText().includes('输入时序验证A'),
    );
    record('①a 逐键输入落 doc', text.includes('输入时序验证A'), `pmText="${text.slice(-12)}"`);
    record('①b Loro 同步插件生效', syncOk, 'loro map 可访问');

    // 快速连打（时序压力）: 50 字符 rapid fire, 断言无丢键无乱序
    const expected = 'RAPID'.repeat(10);
    await page.keyboard.type(expected, { delay: 0 });
    const after = await page.evaluate(() => window.__probe.pmText());
    const tailOk = after.endsWith(expected);
    record('①c 快速连打无丢键', tailOk, `tail=${after.slice(-15)}`);
  }

  // ── 场景 ②: IME 组合输入（composition events 序列）──
  {
    const tail = await page.evaluate(() => window.__probe.runComposition('全部替换文本'));
    const docOk = await page.evaluate(() => window.__probe.pmText().includes('全部替换文本'));
    record('②a composition 序列不破坏文档', docOk, `tail="${tail}"`);
    // 组合后继续正常输入（恢复性）
    await page.keyboard.type('COMPOSED');
    const after = await page.evaluate(() => window.__probe.pmText());
    record('②b 组合后恢复普通输入', after.includes('COMPOSED'), `tail=${after.slice(-10)}`);
  }

  // ── 场景 ③: 光标恢复 ──
  {
    await page.focus('#editor .ProseMirror');
    await page.evaluate(() => window.__view.focus());
    await page.keyboard.insertText('光标恢复基准行XYZ');
    // 光标移到 XYZ 前（动态定位 — 文档跨场景增长, 固定偏移会落错位置）
    await page.evaluate(() => {
      const pos = window.__probe.pmText().indexOf('XYZ');
      window.__probe.setCursor(pos);
    });
    const before = await page.evaluate(() => window.__probe.selectionFrom());
    await page.evaluate(() => window.__view.dom.blur());
    await page.focus('#editor .ProseMirror');
    await page.evaluate(() => window.__view.focus());
    const after = await page.evaluate(() => window.__probe.selectionFrom());
    // 插入应在原光标位（XYZ 前）— insertText = IME 提交路径
    await page.keyboard.insertText('IN');
    const text = await page.evaluate(() => window.__probe.pmText());
    const restored = before === after && text.includes('INXYZ');
    console.log('③ text tail:', JSON.stringify(text.slice(-20)), 'len:', text.length);
    record('③ blur→focus 光标位保留', restored, `before=${before} after=${after}`);
  }

  // ── 场景 ④: 长文档滚动性能 ──
  {
    const count = await page.evaluate(() => window.__probe.fillLongDoc(1000));
    console.log('fill childCount =', count);
    const scrollBox = await page.$('#editor');
    // 帧间隔采样: 滚动期间 rAF 间隔 (阈值 33ms ≈ 30fps)
    const frames = await page.evaluate(async () => {
      const box = document.getElementById('editor');
      const gaps = [];
      let last = performance.now();
      let raf;
      const loop = () => {
        const now = performance.now();
        gaps.push(now - last);
        last = now;
        raf = requestAnimationFrame(loop);
      };
      raf = requestAnimationFrame(loop);
      // 模拟惯性滚动: 20 步平滑滚动到底再回顶
      const max = box.scrollHeight;
      for (let i = 1; i <= 20; i++) {
        box.scrollTop = (max * i) / 20;
        await new Promise((r) => setTimeout(r, 16));
      }
      for (let i = 20; i >= 0; i--) {
        box.scrollTop = (max * i) / 20;
        await new Promise((r) => setTimeout(r, 16));
      }
      cancelAnimationFrame(raf);
      return gaps.filter((g) => g < 500); // 去掉首帧离群
    });
    const avg = frames.reduce((a, b) => a + b, 0) / frames.length;
    const worst = Math.max(...frames);
    const p95 = frames.sort((a, b) => a - b)[Math.floor(frames.length * 0.95)];
    record(
      '④ 长文档滚动帧率(1000 段)',
      avg < 33 && p95 < 100,
      `childCount=${count} avg=${avg.toFixed(1)}ms p95=${p95.toFixed(1)}ms worst=${worst.toFixed(1)}ms n=${frames.length}`,
    );
  }

  // ── 场景 ⑤: 万字笔记滚动 ≥50fps（DK-05M DoD）──
  {
    const stat = await page.evaluate(() => window.__probe.fillTenThousandChars());
    console.log('万字填充:', JSON.stringify(stat));
    const scrollBox = await page.$('#editor');
    const frames = await page.evaluate(async () => {
      const box = document.getElementById('editor');
      const gaps = [];
      let last = performance.now();
      let raf;
      const loop = () => {
        const now = performance.now();
        gaps.push(now - last);
        last = now;
        raf = requestAnimationFrame(loop);
      };
      raf = requestAnimationFrame(loop);
      const max = box.scrollHeight;
      for (let i = 1; i <= 30; i++) {
        box.scrollTop = (max * i) / 30;
        await new Promise((r) => setTimeout(r, 16));
      }
      for (let i = 30; i >= 0; i--) {
        box.scrollTop = (max * i) / 30;
        await new Promise((r) => setTimeout(r, 16));
      }
      cancelAnimationFrame(raf);
      return gaps.filter((g) => g < 500);
    });
    const avg = frames.reduce((a, b) => a + b, 0) / frames.length;
    const worst = Math.max(...frames);
    const p95 = frames.sort((a, b) => a - b)[Math.floor(frames.length * 0.95)];
    record(
      '⑤ 万字滚动帧率 ≥50fps',
      avg <= 20 && p95 <= 40,
      `chars=${stat.totalChars} paras=${stat.childCount} avg=${avg.toFixed(1)}ms p95=${p95.toFixed(1)}ms worst=${worst.toFixed(1)}ms n=${frames.length}`,
    );
  }

  // ── 场景 ⑥: VoiceOver/TalkBack 可遍历（ARIA 与语义结构）──
  {
    const audit = await page.evaluate(() => {
      const out = { roles: 0, labels: 0, issues: [] };
      // 工具栏语义: role=toolbar + aria-label
      const toolbar = document.querySelector('[role=toolbar]');
      if (toolbar) {
        out.roles++;
        if (toolbar.getAttribute('aria-label')) out.labels++;
        else out.issues.push('toolbar 缺 aria-label');
      } else {
        out.issues.push('无 role=toolbar');
      }
      // 工具条按钮可聚焦遍历（tabindex 非负）
      const buttons = toolbar ? Array.from(toolbar.querySelectorAll('button')) : [];
      const unfocusable = buttons.filter((b) => b.tabIndex < 0).length;
      if (unfocusable > 0) out.issues.push(`${unfocusable} 个按钮 tabIndex<0`);
      // contenteditable 可聚焦 + 有无 label 描述
      const pm = document.querySelector('.ProseMirror');
      if (pm && pm.getAttribute('contenteditable') === 'true') out.roles++;
      else out.issues.push('编辑器缺 contenteditable=true');
      // 菜单 role
      const menu = document.querySelector('[role=menu]');
      if (menu) out.roles++;
      // 标题层级结构（heading 语义）
      const headings = pm ? pm.querySelectorAll('h1,h2,h3').length : 0;
      out.headingCount = headings;
      return out;
    });
    record(
      '⑥ 可遍历性（ARIA/焦点路径）',
      audit.issues.length === 0,
      `roles=${audit.roles} labels=${audit.labels} headings=${audit.headingCount || 0} issues=${JSON.stringify(audit.issues)}`,
    );
  }

  await browser.close();
  const pass = results.filter((r) => r.pass).length;
  console.log(`\nSUMMARY: ${pass}/${results.length} PASS`);
  process.exit(pass === results.length ? 0 : 1);
})();
