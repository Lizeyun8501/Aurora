import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { THEME_DEFAULTS, THEME_WHITELIST } from '../theme/themeVm';

/** C-09: 令牌基线三面对拍——tokens.ts 单一真源（ADR-006）。
 *
 * 面一: tokens.ts（apps/desktop/src/design——真源）color 12 键值
 * 面二: themeVm THEME_DEFAULTS（A-11 白名单默认——双端同源 VM）
 * 面三: boot.tsx/main.tsx 双端注入源（C-09 启动注入——include_str
 *       实文件读,与 B-03 Rust 侧 b03_token_mapping_consistency 同
 *       惯例:任何一面漂移即红,单源纪律由测试锁定）
 * 另: mobile.css :root 孤儿投影块 12 键与真源一致（孤儿实化锚点） */
import path from 'node:path';
const here = fileURLToPath(import.meta.url);
const repoRoot = path.resolve(path.dirname(here), '../../../..') + path.sep;

function readRepo(rel: string): string {
  return readFileSync(path.join(repoRoot, rel), 'utf8');
}

/** tokens.ts color 块解析（hex 值抽取——B-03 Rust 测试同法）。 */
function parseTokensColor(src: string): Record<string, string> {
  const seg = src.split('color: {');
  const block = seg.length > 1 ? (seg[1].split('},')[0] ?? '') : '';
  const out: Record<string, string> = {};
  for (const line of block.split('\n')) {
const m = line.match(/^\s*([a-z][a-zA-Z0-9]*)\s*:\s*"(#[0-9a-fA-F]{6})",?\s*(?:\/\/.*)?$/);
    if (m) out[m[1]] = m[2].toLowerCase();
  }
  return out;
}

/** 注入源文件里 setProperty('--k', tokens.color.x) 12 键抽取。 */
function parseInjectKeys(src: string): string[] {
  const out: string[] = [];
  for (const m of src.matchAll(/setProperty\('(--[a-z-]+)'\s*,\s*tokens\.color\.([a-zA-Z]+)\)/g)) {
    out.push(`${m[1]}=${m[2]}`);
  }
  return out;
}

describe('C-09: 令牌基线三面对拍（tokens.ts 单源）', () => {
  const tokensTs = readRepo('apps/desktop/src/design/tokens.ts');
  const color = parseTokensColor(tokensTs);

  it('面一: tokens.ts color 恰 12 键（ADR-006 基线全量）', () => {
    expect(Object.keys(color).sort()).toEqual(
      [
        'bgBase', 'bgSurface', 'bgElevated',
        'primary', 'primaryBright',
        'textPrimary', 'textSecondary', 'textDisabled',
        'success', 'warning', 'danger', 'focus',
      ].sort(),
    );
  });

  it('面一↔面二: tokens.ts 值 = THEME_DEFAULTS（A-11 同源零漂移）', () => {
    for (const k of THEME_WHITELIST) {
      const tsVal = color[k];
      const vmVal = THEME_DEFAULTS[k];
      expect(vmVal?.toLowerCase(), `themeVm ${k} 漂移`).toBe(tsVal);
    }
  });

  it('面三a: 桌面 boot.tsx 注入 12 键（setProperty 全量）', () => {
    const boot = readRepo('apps/desktop/src/boot.tsx');
    const injects = parseInjectKeys(boot);
    expect(injects).toHaveLength(12);
  });

  it('面三b: 移动 main.tsx 注入 12 键（与桌面同值同源）', () => {
    const main = readRepo('apps/mobile/src/main.tsx');
    const injects = parseInjectKeys(main);
    expect(injects).toHaveLength(12);
    // 双端注入逐键一致（同引用 tokens.color——源级断言）
    const boot = readRepo('apps/desktop/src/boot.tsx');
    expect(injects).toEqual(parseInjectKeys(boot));
  });

  it('mobile.css ADR-006 孤儿投影块与真源一致（孤儿实化锚点）', () => {
    const css = readRepo('apps/mobile/src/styles/mobile.css');
    const head = css.slice(0, css.indexOf('}', css.indexOf(':root')) + 1);
    // 12 键逐一: --kebab: #hex 形态在 :root 块内
    const map: Record<string, string> = {
      'bg-base': color.bgBase,
      'bg-surface': color.bgSurface,
      'bg-elevated': color.bgElevated,
      primary: color.primary,
      'primary-bright': color.primaryBright,
      'text-primary': color.textPrimary,
      'text-secondary': color.textSecondary,
      'text-disabled': color.textDisabled,
      success: color.success,
      warning: color.warning,
      danger: color.danger,
      focus: color.focus,
    };
  for (const [k, v] of Object.entries(map)) {
      const want = v?.toLowerCase();
      const got = (head.match(new RegExp(`--${k}:\\s*(#[0-9a-fA-F]{6})`)) ?? [])[1]?.toLowerCase();
      expect(got, `mobile.css --${k} 块漂移`).toBe(want);
    }
  });
});
