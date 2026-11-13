import { Component, StrictMode, type ReactNode } from 'react';
import { createRoot } from 'react-dom/client';
import MobileApp from './MobileApp';
import './styles/mobile.css';
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
        <div style={{ padding: 24, color: '#c0392b', fontSize: 14 }}>
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
