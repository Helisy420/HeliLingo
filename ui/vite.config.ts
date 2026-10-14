import { resolve } from 'node:path';
import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import pkg from './package.json' with { type: 'json' };

// One HTML entry per window; Tauri loads them by file name.
const windows = ['gallery', 'quick', 'tray', 'popup', 'translator', 'settings', 'history', 'ultra', 'welcome', 'capture'];

export default defineConfig({
  plugins: [svelte()],
  define: {
    __APP_VERSION__: JSON.stringify(pkg.version),
    __BUILD_DATE__: JSON.stringify(new Date().toLocaleDateString('ru-RU')),
  },
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: {
    target: 'es2022',
    outDir: 'dist',
    emptyOutDir: true,
    rollupOptions: {
      input: Object.fromEntries(windows.map((w) => [w, resolve(import.meta.dirname, `${w}.html`)])),
    },
  },
});
