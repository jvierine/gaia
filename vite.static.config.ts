import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { resolve } from 'node:path';

export default defineConfig({
  base: '/gaia/',
  plugins: [react()],
  publicDir: 'public',
  build: { outDir: 'web-dist', emptyOutDir: true, rollupOptions:{input:{main:resolve(__dirname,'index.html'),event20251111:resolve(__dirname,'events/20251111/index.html')}} },
});
