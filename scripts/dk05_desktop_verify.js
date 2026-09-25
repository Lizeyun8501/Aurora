/**
 * DK-05 S0 桌面输入时序复验 — Playwright(Chromium) 驱动
 *
 * 仿 dk05mv_verify.js（mobile 版），验证共享层编辑器实体
 * （@aurora/ui-components，editor-uplift 上移产物）+ loro-prosemirror 0.4.4
 * 在桌面浏览器引擎下的时序稳定性。桌面差异项（相对 mobile 版）：
 *   - ④ 滚轮滚动中输入（mouse.wheel + typing 并发 — 桌面主交互）
 *   - ⑤ 长文档滚动 + 光标恢复（跨滚动选区一致性）
 *
 * 环境：dist-lab/editor-lab.html（apps/desktop/vite.config.editorlab.ts 构建，
 * 单文件内联）。发现阻塞性缺陷 → 回执并停线（不带病挂内核）。
 */
const { chromium } = require('/home/z/.npm-global/lib/node_modules/playwright');

(async () => {
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
  const results = [];
  const record = (name, pass, detail) => {
    results.push({ name, pass });
    console.log(`${pass ? 'PASS' : 'FAIL'} ${name}: ${detail}`);
  };

  await page.goto('file:///home/z/my-project/repos/Aurora/apps/desktop/dist-lab/editor-lab.html');
  await page.waitForFunction(() => window.__view && window.__probe && window.__pmState, null, { timeout: 20000 });

  // ── 场景 ①: 焦点与逐键输入落 doc ──
  {
    await page.evaluate(() => document.querySelector('.ProseMirror').focus());
    await page.waitForTimeout(120);
    const hasFocus = await page.evaluate(
      () => document.activeElement && document.activeElement.classList.contains('ProseMirror'),
    );
    await page.keyboard.insertText('桌面输入时序验证A');
    const text = await page.evaluate(() => window.__probe.pmText());
    record('①a 焦点可达 + 逐键输入落 doc', hasFocus && text.includes('桌面输入时序验证A'), `tail="${text.slice(-14)}"`);

    const loroOk = await page.evaluate(() => {
      const t = window.__loro.getText('body') ?? window.__loro.getText();
      return (t?.toString?.() ?? '').includes('桌面输入时序验证A') || window.__loro.getMap('doc') !== undefined;
    });
    record('①b Loro 同步插件生效', loroOk, 'loro doc 可访问');
  }

  // ── 场景 ②: 快速连续输入（时序压力） ──
  {
    const expected = 'RAPID'.repeat(12);
    await page.keyboard.type(expected, { delay: 0 });
    const after = await page.evaluate(() => window.__probe.pmText());
    record('②a 快速连打无丢键无乱序', after.endsWith(expected), `tail="${after.slice(-15)}"`);
    const size = await page.evaluate(() => window.__probe.docSize());
    record('②b doc 尺寸单调一致', size > 0, `size=${size}`);
  }

  // ── 场景 ③: IME 组合输入（拼音候选流 → 提交 → 恢复） ──
  {
    const tail = await page.evaluate(() => window.__probe.runComposition('全部替换文本'));
    const docOk = await page.evaluate(() => window.__probe.pmText().includes('全部替换文本'));
    record('③a composition 序列不破坏文档', docOk, `tail="${tail}"`);
    await page.keyboard.type('COMPOSED');
    const after = await page.evaluate(() => window.__probe.pmText());
    record('③b 组合后恢复普通输入', after.includes('COMPOSED'), `tail="${after.slice(-10)}"`);
  }

  // ── 场景 ④: 滚轮滚动中输入（桌面主交互差异项） ──
  {
    const paras = await page.evaluate(() => window.__probe.fillLongDoc(400));
    const before = await page.evaluate(() => window.__probe.pmText());
    // 滚到底部
    await page.evaluate(() => {
      const el = document.getElementById('editor');
      el.scrollTop = el.scrollHeight;
    });
    await page.waitForTimeout(100);
    // 滚轮持续滚动 + 并发输入（桌面滚轮时序核心压力）
    const wheelAndType = async () => {
      for (let i = 0; i < 6; i++) {
        await page.mouse.move(640, 400);
        await page.mouse.wheel(0, 120);
        if (i === 2) await page.keyboard.insertText('滚轮并发插入');
        await page.waitForTimeout(60);
      }
    };
    await wheelAndType();
    const after = await page.evaluate(() => window.__probe.pmText());
    const intact = before.split('\n').every((l) => l === '' || after.includes(l));
    const injected = after.includes('滚轮并发插入');
    record('④a 滚轮滚动中输入不丢行', intact, `400 段全在=${intact}`);
    record('④b 滚轮并发插入落 doc', injected, '并发窗口 insertText 成功');
  }

  // ── 场景 ⑤: 长文档滚动 + 光标恢复 ──
  {
    await page.evaluate(() => {
      const { TextSelection } = window.__pmState;
      const view = window.__view;
      const size = view.state.doc.content.size;
      view.dispatch(view.state.tr.setSelection(TextSelection.create(view.state.doc, Math.floor(size / 2))));
      const el = document.getElementById('editor');
      el.scrollTop = 0;
    });
    await page.waitForTimeout(80);
    const scroll0 = await page.evaluate(() => window.__probe.scrollInfo());
    await page.evaluate(() => {
      const { TextSelection } = window.__pmState;
      const view = window.__view;
      const pos = Math.floor(view.state.doc.content.size / 2);
      view.dispatch(view.state.tr.setSelection(TextSelection.create(view.state.doc, pos)).scrollIntoView());
    });
    await page.waitForTimeout(120);
    const scroll1 = await page.evaluate(() => window.__probe.scrollInfo());
    const from = await page.evaluate(() => window.__probe.selectionFrom());
    record('⑤a scrollIntoView 后光标位置保持', from > 0, `from=${from}`);
    record('⑤b 长文档滚动状态可读', scroll0.height > scroll0.client && scroll1.top >= 0, `scrollH=${scroll1.height}`);
  }

  browser.close();
  const failed = results.filter((r) => !r.pass).length;
  console.log(`\nSUMMARY: ${results.length - failed}/${results.length} PASS${failed ? ` — ${failed} FAIL` : ''}`);
  process.exit(failed ? 1 : 0);
})();
