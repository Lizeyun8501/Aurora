// R19 lab 独立构建（单文件内联 — MobileEditorDock/BlockInsertPanel Playwright 验证页）。
// 与 editorlab 同模式：避免多入口与 singlefile 的 inlineDynamicImports 冲突。
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { viteSingleFile } from 'vite-plugin-singlefile';
import { fileURLToPath } from 'node:url';

export default defineConfig({
  plugins: [react(), viteSingleFile()],
  resolve: {
    alias: {
      // @aurora/ui-components → 共享层源（React 组件直挂——不经 package 构建产物）
      '@aurora/ui-components': fileURLToPath(new URL('../../shared/ui-components/src/index.ts', import.meta.url)),
    },
  },
  build: {
    outDir: 'dist-r19',
    target: 'esnext',
    assetsInlineLimit: 4_000_000,
    rollupOptions: { input: { 'r19-lab': 'r19-lab.html' } },
  },
});
