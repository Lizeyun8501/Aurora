import { Component, StrictMode, type ReactNode } from 'react';
import { createRoot } from 'react-dom/client';
import MobileApp from './MobileApp';
import './styles/mobile.css';

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
