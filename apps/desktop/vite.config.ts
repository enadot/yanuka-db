import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';
import { readFileSync } from 'node:fs';
import { fileURLToPath, URL } from 'node:url';

const { version } = JSON.parse(
  readFileSync(fileURLToPath(new URL('./package.json', import.meta.url)), 'utf-8'),
) as { version: string };

/**
 * Vite configuration for the desktop frontend.
 *
 * The same bundle serves two hosts: the Tauri webview (production) and a plain
 * browser (development and CI). Which repository backs it is chosen at runtime
 * from `VITE_DATA_SOURCE`, so a machine without a Tauri toolchain can still run
 * and test the real application. See src/lib/repository.tsx.
 */
export default defineConfig({
  plugins: [react(), tailwindcss()],

  // The single source of the version the UI shows. scripts/set-version.mjs
  // keeps package.json aligned with tauri.conf.json (the installer's version),
  // and src/version.test.ts fails the pipeline if they ever drift.
  define: {
    __APP_VERSION__: JSON.stringify(version),
  },

  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },

  // Tauri's dev server contract: a fixed port that must not silently move, and
  // no obfuscation of errors coming from the Rust side.
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      ignored: ['**/src-tauri/**'],
    },
  },

  build: {
    // Edge/WebView2 on Windows is the production target; Chrome 105 covers the
    // baseline that ships with supported Windows versions.
    target: 'chrome105',
    sourcemap: true,
    outDir: 'dist',
  },
});
