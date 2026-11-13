/**
 * A-11（TA 2026-11-14 工单）: 用户自定义主题 VM——设计令牌白名单覆盖（双端同源）。
 *
 * 安全纪律（V26 工单原文「仅允许覆盖设计令牌白名单变量,禁任意 CSS 注入」）:
 * - 白名单=ADR-006 令牌基线 12 色（tokens.ts color 集——与 CSS 变量投影
 *   22 变量名集合锁定对齐）;白名单外令牌（spacing/typography/radius/motion/
 *   a11y/语义衍生）**锁定不可改**
 * - 应用通道=element.style.setProperty 逐变量写（**零字符串 CSS 拼接/零
 *   innerHTML/零 dangerouslySetInnerHTML**——值经 hex 严格校验后才进白名单变量）
 * - 对比度硬约束: textPrimary/bgBase WCAG 相对亮度比 <4.5:1 **拒绝应用**
 *   （不可跳过——工单原文）;primaryBright/bgSurface 联动校验（高亮态可读）
 *
 * 双端: 桌面（KV 持久化+面板）与移动（localStorage+SettingsView）共用本
 * 解析;主题 JSON 同形（导入导出跨端可用）。
 *
 * 停手边界: 不做社区主题市场/不做按工作区独立主题。
 */

/** 白名单令牌键（tokens.ts color 全集——恰 12 项）。 */
export type ThemeTokenKey =
  | 'bgBase'
  | 'bgSurface'
  | 'bgElevated'
  | 'primary'
  | 'primaryBright'
  | 'textPrimary'
  | 'textSecondary'
  | 'textDisabled'
  | 'success'
  | 'warning'
  | 'danger'
  | 'focus';

/** 白名单（冻结——白名单外令牌不可改的运行时依据）。 */
export const THEME_WHITELIST: readonly ThemeTokenKey[] = Object.freeze([
  'bgBase', 'bgSurface', 'bgElevated',
  'primary', 'primaryBright',
  'textPrimary', 'textSecondary', 'textDisabled',
  'success', 'warning', 'danger', 'focus',
]);

/** 令牌 → CSS 变量名（kebab 投影——mobile.css/desktop 变量名集合锁定）。 */
const CSS_VAR: Record<ThemeTokenKey, string> = {
  bgBase: '--bg-base',
  bgSurface: '--bg-surface',
  bgElevated: '--bg-elevated',
  primary: '--primary',
  primaryBright: '--primary-bright',
  textPrimary: '--text-primary',
  textSecondary: '--text-secondary',
  textDisabled: '--text-disabled',
  success: '--success',
  warning: '--warning',
  danger: '--danger',
  focus: '--focus',
};

/** 默认值（暗色基线——tokens.ts 对齐）。 */
export const THEME_DEFAULTS: Record<ThemeTokenKey, string> = {
  bgBase: '#10151C',
  bgSurface: '#162235',
  bgElevated: '#1D2633',
  primary: '#1B6B7A',
  primaryBright: '#37DCF2',
  textPrimary: '#E8EDF2',
  textSecondary: '#9AA7B4',
  textDisabled: '#5A6672',
  success: '#3DDB8E',
  warning: '#E8B33D',
  danger: '#E85D5D',
  focus: '#37DCF2',
};

/** 用户覆盖（仅白名单键——JSON 持久化形）。 */
export type ThemeOverrides = Partial<Record<ThemeTokenKey, string>>;

/** 预设方案（5 套——含默认暗色基线）。 */
export const THEME_PRESETS: Array<{ id: string; name: string; overrides: ThemeOverrides }> = [
  { id: 'aurora-dark', name: '极光暗（默认）', overrides: {} },
  {
    id: 'morning',
    name: '晨光（亮色）',
    overrides: {
      bgBase: '#F7F9FB', bgSurface: '#FFFFFF', bgElevated: '#EEF2F6',
      primary: '#0F6E7E', primaryBright: '#0E7490',
      textPrimary: '#1A2330', textSecondary: '#4B5563', textDisabled: '#9CA3AF',
      focus: '#0E7490',
    },
  },
  {
    id: 'forest',
    name: '墨青',
    overrides: {
      bgBase: '#0D1512', bgSurface: '#13201A', bgElevated: '#1A2A22',
      primary: '#2F7D5D', primaryBright: '#5EE6A8',
      textPrimary: '#E6F2EA', textSecondary: '#94A8A0', focus: '#5EE6A8',
    },
  },
  {
    id: 'dusk',
    name: '暮紫',
    overrides: {
      bgBase: '#16121F', bgSurface: '#1F1930', bgElevated: '#28203D',
      primary: '#6D5BA6', primaryBright: '#B9A6F5',
      textPrimary: '#EDEAF5', textSecondary: '#A79FC0', focus: '#B9A6F5',
    },
  },
  {
    id: 'contrast',
    name: '高对比（无障碍）',
    overrides: {
      bgBase: '#000000', bgSurface: '#0A0A0A', bgElevated: '#141414',
      primary: '#00B8D9', primaryBright: '#00E5FF',
      textPrimary: '#FFFFFF', textSecondary: '#D0D5DD', textDisabled: '#8A8F98',
      focus: '#00E5FF',
    },
  },
];

/** hex 严格校验（#RGB/#RRGGBB——白名单值的唯一合法形;拒绝 rgba()/url()/表达式等注入面）。 */
export function isValidHex(v: string): boolean {
  return /^#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{6})$/.test(v);
}

/** sRGB 通道线性化（WCAG 相对亮度——A-11 对比度校验基）。 */
function channel(c: number): number {
  const s = c / 255;
  return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
}

/** WCAG 相对亮度（0=黑 1=白）。 */
export function relativeLuminance(hex: string): number {
  const h = hex.replace('#', '');
  const full = h.length === 3 ? h.split('').map((c) => c + c).join('') : h;
  const r = parseInt(full.slice(0, 2), 16);
  const g = parseInt(full.slice(2, 4), 16);
  const b = parseInt(full.slice(4, 6), 16);
  return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
}

/** 对比度比（1..21）。 */
export function contrastRatio(fg: string, bg: string): number {
  const l1 = relativeLuminance(fg);
  const l2 = relativeLuminance(bg);
  const [hi, lo] = l1 >= l2 ? [l1, l2] : [l2, l1];
  return (hi + 0.05) / (lo + 0.05);
}

/** 校验结果（拒绝原因可读——应用前全量校验）。 */
export interface ThemeValidation {
  ok: boolean;
  errors: string[];
}

/**
 * 主题校验（应用前）:
 * 1. 白名单外键 → 拒绝（「白名单外令牌不可改」DoD）
 * 2. 值非严格 hex → 拒绝（注入面封死）
 * 3. textPrimary/bgBase 合成对比度 <4.5:1 → 拒绝（正文可读硬约束,不可跳过）
 * 4. primaryBright/bgSurface <3:1 → 警告性拒绝（高亮态可读联动）
 */
export function validateTheme(overrides: ThemeOverrides): ThemeValidation {
  const errors: string[] = [];
  for (const key of Object.keys(overrides)) {
    if (!(THEME_WHITELIST as readonly string[]).includes(key)) {
      errors.push(`「${key}」不在可覆盖白名单（12 项令牌）内——已锁定`);
    }
  }
  for (const [key, v] of Object.entries(overrides)) {
    if (typeof v === 'string' && !isValidHex(v)) {
      errors.push(`「${key}」的值「${v}」不是合法十六进制色（#RGB 或 #RRGGBB）`);
    }
  }
  const merged = { ...THEME_DEFAULTS, ...overrides };
  const bodyRatio = contrastRatio(merged.textPrimary, merged.bgBase);
  if (bodyRatio < 4.5) {
    errors.push(
      `正文对比度 ${bodyRatio.toFixed(2)}:1（textPrimary/bgBase）低于无障碍硬约束 4.5:1——请调深背景或调亮正文`,
    );
  }
  const focusRatio = contrastRatio(merged.primaryBright, merged.bgSurface);
  if (focusRatio < 3) {
    errors.push(
      `高亮态对比度 ${focusRatio.toFixed(2)}:1（primaryBright/bgSurface）低于 3:1——选中/焦点元素将难以辨识`,
    );
  }
  return { ok: errors.length === 0, errors };
}

/**
 * 应用主题（**唯一写入口**）——先校验后写;逐变量 style.setProperty（零字符串
 * CSS 拼接/零 innerHTML）;白名单 12 变量全量写（未覆盖项落默认——保证重置语义）。
 */
export function applyTheme(overrides: ThemeOverrides): ThemeValidation {
  const v = validateTheme(overrides);
  if (!v.ok) return v;
  if (typeof document === 'undefined') return v;
  const merged = { ...THEME_DEFAULTS, ...overrides };
  const root = document.documentElement;
  for (const key of THEME_WHITELIST) {
    root.style.setProperty(CSS_VAR[key], merged[key]);
  }
  return v;
}

/** 清除覆盖（回 ADR-006 基线——逐变量移除内联,回落样式表默认）。 */
export function clearTheme(): void {
  if (typeof document === 'undefined') return;
  const root = document.documentElement;
  for (const key of THEME_WHITELIST) {
    root.style.removeProperty(CSS_VAR[key]);
  }
}

/** 导出（JSON——白名单覆盖项+版本;导入端同 validateTheme 把关）。 */
export function exportThemeJSON(overrides: ThemeOverrides): string {
  return JSON.stringify({ version: 1, type: 'aurora-theme', overrides }, null, 2);
}

/** 导入（解析+校验——白名单外键/非法值/低对比全拒绝,返回可读原因）。 */
export function importThemeJSON(
  raw: string,
): { overrides: ThemeOverrides | null; errors: string[] } {
  try {
    const parsed = JSON.parse(raw) as { version?: number; type?: string; overrides?: unknown };
    if (parsed.type !== 'aurora-theme') {
      return { overrides: null, errors: ['不是 Aurora 主题文件（type 不符）'] };
    }
    if (parsed.version !== 1) {
      return { overrides: null, errors: [`主题版本 ${String(parsed.version)} 不受支持（当前 v1）`] };
    }
    const ov = (parsed.overrides ?? {}) as Record<string, unknown>;
    const cleaned: ThemeOverrides = {};
    for (const [k, v] of Object.entries(ov)) {
      if ((THEME_WHITELIST as readonly string[]).includes(k) && typeof v === 'string') {
        cleaned[k as ThemeTokenKey] = v;
      }
    }
    const v = validateTheme(cleaned);
    if (!v.ok) return { overrides: null, errors: v.errors };
    return { overrides: cleaned, errors: [] };
  } catch {
    return { overrides: null, errors: ['JSON 解析失败——文件损坏或格式不符'] };
  }
}
