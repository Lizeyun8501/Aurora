import { defineConfig } from 'vite';

// Aurora Desktop — Vite 工程（V20 P0-2 / GAP-02）
// 界面 shell 属下阶段（V20 §5），本骨架只保障 dev/build 闭环与 CoreAPI 注入。
export default defineConfig({
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  build: {
    target: 'es2021',
    outDir: 'dist',
  },
});
