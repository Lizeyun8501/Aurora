// DK-2.9v2-b 烟测：popup.js buildCapturePayload 载荷对拍（协议冻结面 c087ec7）。
// 运行：node scripts/dk29b-smoke.mjs（CI 可挂 node 环境——本卡手动跑）
import { createRequire } from 'module';
const require = createRequire(import.meta.url);
const { buildCapturePayload } = require('../apps/browser-extension/popup.js');

let fails = 0;
const eq = (name, a, b) => {
  const ok = JSON.stringify(a) === JSON.stringify(b);
  console.log(`${ok ? 'ok' : 'FAIL'}  ${name}`);
  if (!ok) { fails++; console.log('  got:', JSON.stringify(a), '\n  want:', JSON.stringify(b)); }
};

// 1: 页面值直通
eq('页面值直通', buildCapturePayload({ title: 'T', url: 'https://x/y', content: 'C' }), { title: 'T', url: 'https://x/y', content: 'C' });
// 2: 覆盖优先
eq('覆盖优先', buildCapturePayload({ title: 'T', url: 'u', content: 'C' }, { title: 'X', content: 'Y' }), { title: 'X', url: 'u', content: 'Y' });
// 3: 空串覆盖回落页面值
eq('空串覆盖回落', buildCapturePayload({ title: 'T', url: 'u', content: 'C' }, { title: '', content: '' }), { title: 'T', url: 'u', content: 'C' });
// 4: 页面缺字段兜底空串（schema 契约——三键恒在）
eq('缺字段兜底', buildCapturePayload({}), { title: '', url: '', content: '' });

process.exit(fails ? 1 : 0);
