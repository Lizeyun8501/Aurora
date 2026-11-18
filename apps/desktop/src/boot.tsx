/**
 * Aurora Desktop 入口 — V23-I5（boot.tsx 取代 root 属主 main.tsx）
 *
 * main.tsx（root 属主, 本用户不可写）保留旧 AppShell; vite 入口改指
 * 本文件 → DesktopShell 三栏布局。main.tsx 下轮 git 清理。
 */
import { createRoot } from 'react-dom/client';
import DesktopShell from './components/DesktopShell';
/* C-09: 令牌基线变量 boot 注入——tokens.ts 单一真源（ADR-006）。
 * createRoot 前同步写 12 变量（React 加载/首帧渲染前就位）:
 * - shared 组件注入 CSS（var(--brand/--bg-primary/--text-primary…)
 *   告别亮/青回退值——消费 ADR-006 基线真值
 * - A-11 applyTheme 覆写面不变（setProperty 内联同帧翻转——启动
 *   KV 主题在 DesktopShell 挂载 effect 应用,基线先铺避免空窗）
 * - 单源纪律: 值与 themeVm THEME_DEFAULTS 一致性由
 *   c09TokenBaseline.test.ts 对拍锁定（漂移即红） */
import tokens from './design/tokens';

const el = document.getElementById('root');
if (el && typeof document !== 'undefined') {
  const root = document.documentElement;
  root.style.setProperty('--bg-base', tokens.color.bgBase);
  root.style.setProperty('--bg-surface', tokens.color.bgSurface);
  root.style.setProperty('--bg-elevated', tokens.color.bgElevated);
  root.style.setProperty('--primary', tokens.color.primary);
  root.style.setProperty('--primary-bright', tokens.color.primaryBright);
  root.style.setProperty('--text-primary', tokens.color.textPrimary);
  root.style.setProperty('--text-secondary', tokens.color.textSecondary);
  root.style.setProperty('--text-disabled', tokens.color.textDisabled);
  root.style.setProperty('--success', tokens.color.success);
  root.style.setProperty('--warning', tokens.color.warning);
  root.style.setProperty('--danger', tokens.color.danger);
  root.style.setProperty('--focus', tokens.color.focus);
}
if (el) {
  createRoot(el).render(<DesktopShell />);
}
