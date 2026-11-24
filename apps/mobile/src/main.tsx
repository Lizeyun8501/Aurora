import { Component, StrictMode, type ReactNode } from 'react';
import { createRoot } from 'react-dom/client';
import MobileApp from './MobileApp';
import './styles/mobile.css';
/* C-09: 令牌基线变量启动注入——tokens.ts 单一真源（ADR-006）。
 * createRoot 前同步写 12 变量（与桌面 boot.tsx 同源注入——值由
 * c09TokenBaseline.test.ts 对拍锁定漂移即红）;移动侧作用:
 * - mobile.css ADR-006 孤儿投影块（:root 12 键）由内联注入实值化
 *   ——变量不再是纯声明,真有消费链（shared 组件/桌面同源面）
 * - A-11 applyTheme（SettingsView）覆写面不变——同帧翻转
 * - V19 面板变量（--brand/--bg-primary 等）不受影响（命名系独立,
 *   收敛=另卡;本注入只实化 ADR-006 12 键基线） */
import tokens from '@aurora/design-tokens';

if (typeof document !== 'undefined') {
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
/* A-11: 启动应用持久化主题（白名单覆盖——存储态直应用;校验过的数据
 * 重入校验防篡改,失败静默清——恢复默认不阻断启动）。 */
import { applyTheme, validateTheme, type ThemeOverrides } from '@aurora/ui-components';
try {
  const ov = JSON.parse(localStorage.getItem('aurora.theme.custom') ?? '{}') as ThemeOverrides;
  if (Object.keys(ov).length > 0 && validateTheme(ov).ok) applyTheme(ov);
} catch { /* 损坏/隐私模式——默认主题 */ }

const rootElement = document.getElementById('root');
if (!rootElement) {
  throw new Error('Root element #root not found');
}

/** 根错误边界 — 渲染异常显示提示而非白屏（v10 白屏事故防御）。 */
class AppErrorBoundary extends Component<{ children: ReactNode }, { error: Error | null }> {
  state = { error: null as Error | null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  componentDidCatch(error: Error) {
    console.error('App render error:', error);
  }

  render() {
    if (this.state.error) {
      return (
        <div style={{ padding: 24, color: 'var(--danger)', fontSize: 14 }}>
          界面渲染出错：{this.state.error.message}
          <br />
          <button
            style={{ marginTop: 12, padding: '8px 16px' }}
            onClick={() => location.reload()}
          >
            重新加载
          </button>
        </div>
      );
    }
    return this.props.children;
  }
}

createRoot(rootElement).render(
  <StrictMode>
    <AppErrorBoundary>
      <MobileApp />
    </AppErrorBoundary>
  </StrictMode>,
);
