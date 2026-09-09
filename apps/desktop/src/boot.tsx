/**
 * Aurora Desktop 入口 — V23-I5（boot.tsx 取代 root 属主 main.tsx）
 *
 * main.tsx（root 属主, 本用户不可写）保留旧 AppShell; vite 入口改指
 * 本文件 → DesktopShell 三栏布局。main.tsx 下轮 git 清理。
 */
import { createRoot } from 'react-dom/client';
import DesktopShell from './components/DesktopShell';

const el = document.getElementById('root');
if (el) {
  createRoot(el).render(<DesktopShell />);
}
