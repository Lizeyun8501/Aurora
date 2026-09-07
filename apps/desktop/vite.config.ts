import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// Aurora Desktop — Vite 工程（V20 P0-2 骨架 + V23-I1 React Shell）
// 双端统一组件策略: 桌面/移动共享 React + Design Tokens（src/design/tokens.ts）
export default defineConfig({
  clearScreen: false,
  plugins: [react()],
  server: {
    port: 1420,
    strictPort: true,
  },
  build: {
    target: 'es2021',
    outDir: 'dist',
  },
});
