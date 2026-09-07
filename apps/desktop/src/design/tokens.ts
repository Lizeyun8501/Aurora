/**
 * Aurora Design Tokens 基线 — V23-I1（双端共享）
 *
 * 单一事实源：桌面（Tauri/React）与移动（Capacitor/React）引用同一份
 * tokens；Rust 侧主题结构体（aurora-core theme）与本文件字段一一对应。
 *
 * 设计原则（V22.1 §7 承继 + V23-P1 补全）：
 * - 暗色为默认主题：低亮度深空底（非纯黑，降 OLED 频闪疲劳）
 * - 颜色不作为唯一信息载体（状态同时用图标 + 文字）
 * - 正文对比度 ≥ 4.5:1
 * - 动效时长曲线固定三档（100/200/300ms），缓出 easing
 */

export const tokens = {
  color: {
    /** 深空底（暗色默认） */
    bgBase: "#10151C",
    bgSurface: "#162235",
    bgElevated: "#1D2633",
    /** 主色 — 与 V22.1 DM-1 色板对齐（表格用暗化变体） */
    primary: "#1B6B7A",
    primaryBright: "#37DCF2",
    textPrimary: "#E8EDF2",
    textSecondary: "#9AA7B4",
    textDisabled: "#5A6672",
    /** 语义色（图标+文字双载体） */
    success: "#3DDB8E",
    warning: "#E8B33D",
    danger: "#E85D5D",
    /** 高亮态（选中/焦点） */
    focus: "#37DCF2",
  },
  spacing: {
    /** 4pt 网格 */
    xs: 4,
    sm: 8,
    md: 16,
    lg: 24,
    xl: 32,
  },
  typography: {
    body: { size: 15, lineHeight: 1.6, weight: 400 },
    bodyStrong: { size: 15, lineHeight: 1.6, weight: 600 },
    title: { size: 20, lineHeight: 1.4, weight: 700 },
    heading: { size: 17, lineHeight: 1.5, weight: 600 },
    caption: { size: 12, lineHeight: 1.5, weight: 400 },
    /** 中文优先字体栈 */
    family:
      "-apple-system, 'PingFang SC', 'Microsoft YaHei', 'Noto Sans CJK SC', sans-serif",
    mono: "'Sarasa Mono SC', 'JetBrains Mono', 'Cascadia Code', monospace",
  },
  radius: { sm: 4, md: 8, lg: 12 },
  motion: {
    fast: 100,
    normal: 200,
    slow: 300,
    easing: "cubic-bezier(0.2, 0.8, 0.2, 1)",
  },
  /** 无障碍（V22.1 §6.6 承继） */
  a11y: {
    minContrast: 4.5,
    minTouchTarget: 44,
    focusRingWidth: 2,
  },
} as const;

export type DesignTokens = typeof tokens;
export default tokens;
