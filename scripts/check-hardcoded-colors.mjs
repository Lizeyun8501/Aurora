#!/usr/bin/env node
/**
 * U-02/ADR-006: 硬编码颜色静态门禁——扫全仓 TS/TSX/CSS 的
 * hex（#RGB/#RRGGBB…）与 rgba( 使用面：
 *   - 合法定义面（豁免）：tokens.ts / themeVm.ts（令牌单一来源）、
 *     mobile.css 的 :root/[data-theme] 定义段（CSS 变量落地点）、
 *     *.test.* / *.mjs 脚本自身
 *   - 使用面违规：定义面之外的任何 hex / rgba( —— 必须走 var(--*)
 *   - 行级豁免：行注释含 `u02-allow`（须带原因——如平台约束 meta
 *     theme-color 不支持 CSS 变量）
 *   - 纯数字 #NNN（如 React #310 issue 号）不算颜色——正则要求
 *     十六进制串必须含 a-f 字符
 * 分段门禁：移动端（apps/mobile）使用面已清零=硬线阻断;
 * 桌面 TSX 面存量 548 处按卡分批清偿（阈值 600 只防恶化）,
 * 清零后升全量阻断。
 */
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join, dirname, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const ALLOW = [
  'src/design/tokens.ts',
  'shared/ui-components/src/theme/themeVm.ts',
  'scripts/check-hardcoded-colors.mjs',
];
const findings = [];
let scanned = 0;

function cssDefinitionRanges(src) {
  const ranges = [];
  const blockRe = /(:root|\[data-theme=['"][a-z]+['"]\])\s*\{/g;
  for (const m of src.matchAll(blockRe)) {
    let depth = 0, i = src.indexOf('{', m.index);
    for (; i < src.length; i++) {
      if (src[i] === '{') depth++;
      else if (src[i] === '}') { depth--; if (depth === 0) break; }
    }
    ranges.push([m.index, i + 1]);
  }
  return ranges;
}
function inRanges(ranges, x) {
  return ranges.some(([a, b]) => x >= a && x < b);
}

function walk(dir) {
  for (const name of readdirSync(dir)) {
    if (name === 'target' || name === 'node_modules' || name === 'dist' || name.startsWith('.')) continue;
    const full = join(dir, name);
    const st = statSync(full);
    if (st.isDirectory()) { walk(full); continue; }
    if (!/\.(tsx?|css)$/.test(name) || name.includes('.test.')) continue;
    const rel = relative(root, full).replaceAll('\\', '/');
    if (ALLOW.some((a) => rel.endsWith(a))) continue;
    scanned++;
    const src = readFileSync(full, 'utf8');
    const defRanges = rel.endsWith('.css') ? cssDefinitionRanges(src) : [];
    const lines = src.split('\n');
    // hex 必须含 a-f 字符（排除 React #310 类 issue 号）
    const HEX = /#(?:[0-9A-Fa-f]*[a-fA-F][0-9A-Fa-f]*)(?![0-9A-Za-z])/g;
    const RGBA = /rgba?\(\s*\d/g;
    for (const re of [HEX, RGBA]) {
      for (const m of src.matchAll(re)) {
        if (inRanges(defRanges, m.index)) continue;
        const line = src.slice(0, m.index).split('\n').length;
        if ((lines[line - 1] || '').includes('u02-allow')) continue;
        findings.push({ rel, line, hit: m[0] });
      }
    }
  }
}

for (const d of ['apps', 'shared', 'crates']) walk(join(root, d));

const byFile = {};
for (const f of findings) byFile[f.rel] = (byFile[f.rel] || 0) + 1;
for (const [rel, n] of Object.entries(byFile).sort((a, b) => b[1] - a[1]).slice(0, 15))
  console.log(`❌ ${rel}: ${n} 处`);
for (const f of findings.slice(0, 20)) console.log(`   ${f.rel}:${f.line}  ${f.hit}`);
console.log(`硬编码颜色扫描: ${scanned} 文件 · 违规 ${findings.length} 处（合法定义面: tokens.ts/themeVm.ts/CSS :root 段豁免）`);
const mobileFindings = findings.filter((f) => f.rel.startsWith('apps/mobile'));
const DESKTOP_WARN_THRESHOLD = 600;
if (mobileFindings.length > 0) {
  console.log(`❌ 移动端使用面违规 ${mobileFindings.length} 处（U-02 已清零——此为回归,阻断）`);
  process.exit(1);
}
if (findings.length > DESKTOP_WARN_THRESHOLD) {
  console.log(`❌ 总违规 ${findings.length} > 阈值 ${DESKTOP_WARN_THRESHOLD}（桌面基线 548,只增不减——阻断防恶化）`);
  process.exit(1);
}
if (findings.length > 0) {
  console.log(`⚠️  桌面面存量 ${findings.length} 处（≤阈值 600——U-02 第二/三段+U-01 拆分联动清偿中,清零后本门禁升全量阻断）`);
} else {
  console.log('✅ 零硬编码颜色违规（全量阻断模式）');
}
