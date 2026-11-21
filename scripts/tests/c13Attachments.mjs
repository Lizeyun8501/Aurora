#!/usr/bin/env node
/**
 * C-13 附件预览 mock 全链——PW file:// 驱动（B-05 e2eTopPaths 同通道）。
 * 断言面: attachment:// 引用条渲染（4 chip）→ 点击开预览 sheet →
 * 三类 MIME 各一（image data: 渲染/pdf 降级卡/text 分片 pre）+
 * 失败面（不存在 id→显式错误条非静默）。
 */
import { createRequire } from 'module';
import { fileURLToPath } from 'url';
import path from 'path';
let chromium;
try {
  ({ chromium } = createRequire(import.meta.url)('playwright'));
} catch {
  ({ chromium } = createRequire('/home/z/.npm-global/lib/node_modules/playwright/')('playwright'));
}
const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const BASE = process.env.AURORA_E2E_BASE
  ?? 'file://' + path.join(repoRoot, 'apps', 'mobile', 'dist', 'index.html');

let pass = 0, fail = 0;
const ok = (name, cond, extra = '') => {
  if (cond) { pass++; console.log(`PASS ${name}`); }
  else { fail++; console.log(`FAIL ${name}${extra ? ' — ' + extra : ''}`); }
};
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// 1x1 PNG（最小合法——image pane data: 渲染断言用）
const PNG1X1 = 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==';

(async () => {
  const browser = await chromium.launch();
  const ctx = await browser.newContext({ viewport: { width: 390, height: 844 }, hasTouch: true });
  const page = await ctx.newPage();
  const errors = [];
  page.on('pageerror', (e) => errors.push(String(e)));

  await page.goto(BASE);
  await sleep(1600);

  /* ── 造数: 笔记含四条引用（3 真 + 1 missing）+ attachments 三 DTO ── */
  const NID = 'note-c13-e2e';
  await page.evaluate(([nid, png]) => {
    const content = [
      '# 附件',
      '![shot.png](attachment://at-c13-img-001)',
      '[manual.pdf](attachment://at-c13-pdf-001)',
      '[log.txt](attachment://at-c13-txt-001)',
      '[missing.bin](attachment://at-c13-missing-1)',
    ].join('\n');
    localStorage.setItem('aurora.notes', JSON.stringify([
      { id: nid, title: 'C13 附件全链', updatedAt: new Date().toISOString(), content },
    ]));
    localStorage.setItem('aurora.attachments', JSON.stringify([
      { attachment_id: 'at-c13-img-001', note_id: nid, file_name: 'shot.png', mime: 'image/png', size: 70, data_base64: png },
      { attachment_id: 'at-c13-pdf-001', note_id: nid, file_name: 'manual.pdf', mime: 'application/pdf', size: 20, data_base64: btoa('%PDF-1.4 C13 e2e') },
      { attachment_id: 'at-c13-txt-001', note_id: nid, file_name: 'log.txt', mime: 'text/plain', size: 26, data_base64: btoa('C13 text attachment e2e assert') },
    ]));
  }, [NID, PNG1X1]);

  /* ── 打开笔记（对齐 B-05 P2: 直点行 div;快照预置同新建链落库形态） ── */
  await page.evaluate((nid) => {
    // DK-05M: 无桥环境快照 mock——空 LoroDoc snapshot（编辑器挂载前置）
    localStorage.setItem(`aurora.snap.${nid}`,
      'bG9ybwAAAAAAAAAAAAAAAFMuDPwAAy8AAABMT1JPAAAAAgB2dgAAAAEAAgDnYslsAQAAAAUAAAACAGZyAAIAdnbb2MnIFgAAAAAAAAAAAAAA');
  }, [NID]);
  await page.click('.tab-bar .tab-item:has-text("笔记")');
  await sleep(700);
  await page.click('.nt-arrow').catch(() => {});
  await sleep(400);
  await page.click(`.nt-row:has-text("C13 附件全链")`);
  await page.waitForSelector('.ProseMirror', { timeout: 10000 });
  await sleep(800);

  /* ── A1: 附件条渲染（4 chip——3 真 + 1 missing 引用同渲染） ── */
  const chips = page.locator('button[aria-label^="预览附件"]');
  ok('A1 附件条 4 chip 渲染', await chips.count() === 4, `count=${await chips.count()}`);

  /* ── A2: image 预览（data: img 渲染） ── */
  await chips.filter({ hasText: 'shot.png' }).first().click();
  await sleep(900);
  ok('A2a image sheet 开', await page.locator('.sheet-title:has-text("附件预览")').isVisible().catch(() => false));
  const img = page.locator('.sheet img');
  ok('A2b image blob img 渲染', await img.count() >= 1 && (await img.first().getAttribute('src') ?? '').startsWith('blob:'), 'src=blob:（b64→Blob 标准链）');
  await page.click('.sheet .icon-btn[aria-label="关闭"]');
  await sleep(400);

  /* ── A3: pdf 预览（降级卡——PDF_BUDGET_NOTE 移动预算口径） ── */
  await chips.filter({ hasText: 'manual.pdf' }).first().click();
  await sleep(900);
  ok('A3 pdf 降级卡', await page.locator('.sheet .aurora-ap-pdf').isVisible().catch(() => false));
  await page.click('.sheet .icon-btn[aria-label="关闭"]');
  await sleep(400);

  /* ── A4: text 预览（分片 pre + 内容断言） ── */
  await chips.filter({ hasText: 'log.txt' }).first().click();
  await sleep(1200);
  const pre = page.locator('.sheet pre[aria-label="附件文本内容（只读）"]');
  ok('A4 text pre 渲染', await pre.isVisible().catch(() => false));
  ok('A4b text 内容一致', (await pre.textContent().catch(() => '')).includes('C13 text attachment e2e assert'));
  await page.click('.sheet .icon-btn[aria-label="关闭"]');
  await sleep(400);

  /* ── A5: 失败面（missing id→显式错误条——fail-closed 非静默） ── */
  await chips.filter({ hasText: 'missing.bin' }).first().click();
  await sleep(900);
  const err = page.locator('.sheet [role="alert"]');
  ok('A5 missing 显式错误条', await err.isVisible().catch(() => false), (await err.textContent().catch(() => '') ?? '').slice(0, 40));

  ok('A0 无页面错误', errors.length === 0, errors.slice(0, 2).join('; '));
  await browser.close();
  console.log(`\nC-13 E2E: ${pass} pass / ${fail} fail`);
  process.exit(fail > 0 ? 1 : 0);
})();
