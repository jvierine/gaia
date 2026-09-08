import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  base: '/gaia/',
  plugins: [react()],
  publicDir: 'public',
  build: { outDir: 'web-dist', emptyOutDir: true },
});
