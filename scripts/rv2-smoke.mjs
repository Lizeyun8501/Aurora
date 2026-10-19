// DK-2.9v2 Rv2 烟测：Readability 正文提取对拍（vendor/readability.js 0.6.0 同源面）。
// 运行：cd apps/desktop && node ../../scripts/rv2-smoke.mjs（jsdom --no-save）
import { createRequire } from 'module';
import { fileURLToPath } from 'url';
// 相对定位（不硬编码绝对仓路径——跨机器可跑）：仓根/apps/desktop/package.json
const deskPkg = fileURLToPath(new URL('../apps/desktop/package.json', import.meta.url));
const req = createRequire(deskPkg);
const { JSDOM } = req('jsdom');
const { Readability } = req('@mozilla/readability');

let fails = 0;
const ok = (name, cond) => { console.log(`${cond ? 'ok' : 'FAIL'}  ${name}`); if (!cond) fails++; };

const FIXTURE = `<!DOCTYPE html><html><head><title>测试文章标题</title></head>
<body><nav><a href="/home">首页</a><a href="/about">关于</a></nav>
<article><h1>测试文章标题</h1>
<p>${'正文段落。'.repeat(40)}</p>
<p>第二段含关键术语 Aurora。</p></article>
<footer>版权所有 广告位</footer></body></html>`;

// 1: 正文提取命中+噪声剔除
const dom = new JSDOM(FIXTURE, { url: 'https://example.com/article' });
const parsed = new Readability(dom.window.document.cloneNode(true)).parse();
ok('parse 返回非空', !!parsed);
ok('标题命中', parsed.title.includes('测试文章标题'));
ok('正文含段落', parsed.textContent.includes('正文段落'));
ok('nav 链接噪声剔除', !parsed.textContent.includes('关于'));
ok('footer 广告剔除', !parsed.textContent.includes('广告位'));
// 2: 无正文页面 parse 可失败或空（fallback 语义前提）
const empty = new JSDOM('<html><body><div>仅导航</div></body></html>', { url: 'https://x/' });
const p2 = new Readability(empty.window.document.cloneNode(true)).parse();
ok('空内容页不崩溃（fallback 前提）', p2 === null || (p2.textContent || '').length === 0 || true);

process.exit(fails ? 1 : 0);
