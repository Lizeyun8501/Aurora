import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';
import { fileURLToPath } from 'node:url';

// https://vitest.dev/config/
export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      '@aurora/shared-types': fileURLToPath(
        new URL('../types/src/index.ts', import.meta.url),
      ),
    },
  },
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: ['./src/test-setup.ts'],
    include: ['src/**/*.{test,spec}.{ts,tsx}'],
  },
});
