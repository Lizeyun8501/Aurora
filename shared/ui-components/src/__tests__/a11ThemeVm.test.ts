import { describe, it, expect } from 'vitest';
import {
  THEME_WHITELIST,
  THEME_DEFAULTS,
  THEME_PRESETS,
  isValidHex,
  contrastRatio,
  validateTheme,
  exportThemeJSON,
  importThemeJSON,
  type ThemeOverrides,
} from '../theme/themeVm';

/**
 * A-11（TA 2026-11-14 工单）行为级断言:
 * - 白名单恰 12 项+白名单外键拒绝（DoD「白名单外令牌不可改」）
 * - 对比度 <4.5:1 拒绝+可读原因（DoD 硬约束）
 * - hex 严格校验（注入面: rgba/url/表达式全拒）
 * - JSON 导入导出往返+损坏/非主题文件拒绝
 * - 预设 5 套全过校验（应用即验的完整性前提）
 */

describe('themeVm', () => {
  it('白名单恰 12 项（tokens.ts color 全集）', () => {
    expect(THEME_WHITELIST.length).toBe(12);
    expect(THEME_WHITELIST).toContain('bgBase');
    expect(THEME_WHITELIST).toContain('primary');
    expect(THEME_WHITELIST).toContain('textPrimary');
  });

  it('白名单外键拒绝（spacing/typography/accent 锁定）', () => {
    const v = validateTheme({ accent: '#123456' } as unknown as ThemeOverrides);
    expect(v.ok).toBe(false);
    expect(v.errors.some((e) => e.includes('白名单'))).toBe(true);
    const v2 = validateTheme({ spacing: '4px' } as unknown as ThemeOverrides);
    expect(v2.ok).toBe(false);
  });

  it('非法值拒绝（注入面封死——rgba/url/表达式/伪协议）', () => {
    expect(isValidHex('#AABBCC')).toBe(true);
    expect(isValidHex('#ABC')).toBe(true);
    expect(isValidHex('rgba(0,0,0,1)')).toBe(false);
    expect(isValidHex('url(javascript:alert(1))')).toBe(false);
    expect(isValidHex('red; } body { display:none')).toBe(false);
    expect(isValidHex('#GGGGGG')).toBe(false);
    const v = validateTheme({ bgBase: 'javascript:alert(1)' });
    expect(v.ok).toBe(false);
    expect(v.errors.some((e) => e.includes('十六进制'))).toBe(true);
  });

  it('低对比度拒绝+可读原因（正文硬约束 4.5:1）', () => {
    // 深灰正文×深底——低对比
    const v = validateTheme({ textPrimary: '#4A4A4A' });
    expect(v.ok).toBe(false);
    expect(v.errors.some((e) => e.includes('4.5:1'))).toBe(true);
    expect(v.errors.some((e) => e.includes('对比度'))).toBe(true);
  });

  it('高亮态联动校验（primaryBright/bgSurface <3:1 拒）', () => {
    const v = validateTheme({ primaryBright: '#152A35' });
    expect(v.ok).toBe(false);
    expect(v.errors.some((e) => e.includes('高亮态'))).toBe(true);
  });

  it('合法覆盖通过（默认基线全量覆盖零错）', () => {
    const v = validateTheme({});
    expect(v.ok).toBe(true);
    const v2 = validateTheme({ primary: '#2F7D5D', bgBase: '#0D1512' });
    expect(v2.ok).toBe(true);
  });

  it('JSON 导出导入往返（跨端同形）', () => {
    const ov: ThemeOverrides = { primary: '#6D5BA6', bgBase: '#16121F' };
    const json = exportThemeJSON(ov);
    const back = importThemeJSON(json);
    expect(back.errors).toEqual([]);
    expect(back.overrides).toEqual(ov);
  });

  it('导入拒绝面: 非主题文件/坏 JSON/低对比主题全拒', () => {
    expect(importThemeJSON('{"foo":1}').overrides).toBeNull();
    expect(importThemeJSON('not json').overrides).toBeNull();
    expect(importThemeJSON('{"version":1,"type":"aurora-theme","overrides":{"textPrimary":"#4A4A4A"}}').overrides).toBeNull();
    // 注入面经导入链同样拒绝
    const inj = importThemeJSON('{"version":1,"type":"aurora-theme","overrides":{"bgBase":"url(x)"}}');
    expect(inj.overrides).toBeNull();
  });

  it('对比度计算基准（黑白 21:1 量级）', () => {
    expect(contrastRatio('#FFFFFF', '#000000')).toBeCloseTo(21, 0);
    expect(contrastRatio('#FFFFFF', '#FFFFFF')).toBeCloseTo(1, 5);
  });

  it('预设 5 套全过校验', () => {
    expect(THEME_PRESETS.length).toBeGreaterThanOrEqual(3);
    expect(THEME_PRESETS.length).toBeLessThanOrEqual(5);
    for (const p of THEME_PRESETS) {
      const v = validateTheme(p.overrides);
      expect(v.ok, `预设「${p.name}」未过校验: ${v.errors.join(';')}`).toBe(true);
    }
  });

  it('默认值与 ADR-006 基线一致', () => {
    expect(THEME_DEFAULTS.bgBase.toUpperCase()).toBe('#10151C');
    expect(THEME_DEFAULTS.primary.toUpperCase()).toBe('#1B6B7A');
  });
});
