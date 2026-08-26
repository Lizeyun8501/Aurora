import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { viteSingleFile } from 'vite-plugin-singlefile';

// Mobile WebView bundle — V19 §36.3 Capacitor+React 架构。
// 产物注入 Android assets/，由 MainActivity WebView 加载。
// singlefile: 内联 JS/CSS 到单个 HTML — 规避 file:// 下 ES module CORS 限制。
export default defineConfig({
  plugins: [react(), viteSingleFile()],
  resolve: {
    alias: {
      '@aurora/shared-types': new URL(
        '../../shared/types/src/index.ts',
        import.meta.url,
      ).pathname,
      '@aurora/ui-components': new URL(
        '../../shared/ui-components/src/index.ts',
        import.meta.url,
      ).pathname,
    },
  },
  build: {
    outDir: 'dist',
    // esnext: loro-crdt (wasm-bindgen bundler) 使用 top-level await
    target: 'esnext',
    // 内联 wasm (3.2MB) 为 data: URL — file:// WebView 无法 fetch 本地 wasm 资产，
    // data URL 走 fetch + instantiateStreaming 路径（loro bundler 初始化已支持）
    assetsInlineLimit: 4_000_000,
  },
});
