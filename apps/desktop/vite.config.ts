import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// Aurora Desktop — Vite 工程（V20 P0-2 骨架 + V23-I1 React Shell）
// 双端统一组件策略: 桌面/移动共享 React + Design Tokens（src/design/tokens.ts）
export default defineConfig({
  clearScreen: false,
  plugins: [react()],
  resolve: {
    alias: {
      '@aurora/ui-components': new URL(
        '../../shared/ui-components/src/index.ts',
        import.meta.url,
      ).pathname,
    },
  },
  server: {
    port: 1420,
    strictPort: true,
    // 允许 dev 动态 import 仓库根内文件（monorepo 共享层 alias 指向 root 外——
    // 动态 import 走 @fs URL，默认 allow 只覆盖 vite root）
    fs: { allow: [new URL('../../', import.meta.url).pathname] },
  },
  // dev 模式 esbuild target 同步 esnext — loro-crdt wasm 需要 top-level await
  // （build.target 只管构建；dev 转换/依赖预打包分别走 esbuild/optimizeDeps）
  esbuild: { target: 'esnext' },
  optimizeDeps: { esbuildOptions: { target: 'esnext' } },
  build: {
    // esnext: loro-crdt (wasm-bindgen bundler) 使用 top-level await
    // （DK-05 S1 编辑器接入 — 与 apps/mobile/vite.config.ts 同理由）
    target: 'esnext',
    outDir: 'dist',
    // 内联 wasm (3.2MB) 为 data: URL — 桌面 WebView 资产加载同 mobile 约束
    assetsInlineLimit: 4_000_000,
  },
});
