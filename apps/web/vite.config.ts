import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      '@aurora/shared-types': new URL(
        '../../shared/types/src/index.ts',
        import.meta.url,
      ).pathname,
    },
  },
});
