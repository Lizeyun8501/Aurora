// DK-05M-V editor-lab 独立构建（单文件内联 — 与主 App 构建分离,
// 避免多入口与 singlefile 的 inlineDynamicImports 冲突）。
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { viteSingleFile } from 'vite-plugin-singlefile';

export default defineConfig({
  plugins: [react(), viteSingleFile()],
  build: {
    outDir: 'dist-lab',
    target: 'esnext',
    assetsInlineLimit: 4_000_000,
    rollupOptions: { input: { 'editor-lab': 'editor-lab.html' } },
  },
});
