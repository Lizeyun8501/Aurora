// DK-05 S0 editor-lab 独立构建（单文件内联 — 与主 App 构建分离，
// 避免多入口冲突；仿 apps/mobile/vite.config.editorlab.ts 先例）。
// alias 指向共享层编辑器实体（editor-uplift 卡上移产物）——S0 顺带验证
// 「共享层实体在桌面构建栈可解析可运行」。
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { viteSingleFile } from 'vite-plugin-singlefile';
import { fileURLToPath } from 'node:url';

export default defineConfig({
  plugins: [react(), viteSingleFile()],
  resolve: {
    alias: {
      '@aurora/ui-components': fileURLToPath(
        new URL('../../shared/ui-components/src/index.ts', import.meta.url),
      ),
    },
  },
  build: {
    outDir: 'dist-lab',
    target: 'esnext',
    assetsInlineLimit: 4_000_000,
    rollupOptions: { input: { 'editor-lab': 'editor-lab.html' } },
  },
});
