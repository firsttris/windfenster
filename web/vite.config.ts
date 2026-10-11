/// <reference types="vitest/config" />
import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';
import { tanstackRouter } from '@tanstack/router-plugin/vite';
import { paraglideVitePlugin } from '@inlang/paraglide-js';

// In dev the Rust server runs on :8080 and Vite proxies the API to it (`pnpm dev`).
export default defineConfig({
  plugins: [
    // messages/{de,en}.json → src/paraglide: one typed function per text (m.nav_forecast()).
    // The language: the one picked in the app, else the browser's, else German.
    paraglideVitePlugin({
      project: './project.inlang',
      outdir: './src/paraglide',
      strategy: ['localStorage', 'preferredLanguage', 'baseLocale'],
    }),
    // src/routes/* → src/routeTree.gen.ts; has to run before the React plugin.
    tanstackRouter({ target: 'react', autoCodeSplitting: true }),
    react(),
    tailwindcss(),
  ],
  resolve: {
    alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) },
  },
  server: {
    proxy: {
      '/api': { target: process.env.WF_API ?? 'http://127.0.0.1:8080' },
    },
  },
  build: { outDir: 'dist', emptyOutDir: true },
  test: { include: ['src/**/*.test.ts'] },
});
