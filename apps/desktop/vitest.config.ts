import { readFileSync } from 'node:fs';
import { fileURLToPath, URL } from 'node:url';
import { defineConfig } from 'vitest/config';

const { version } = JSON.parse(
  readFileSync(fileURLToPath(new URL('./package.json', import.meta.url)), 'utf-8'),
) as { version: string };

/**
 * Unit-test configuration for the desktop app.
 *
 * `e2e/` is excluded because those specs are driven by Playwright, which
 * provides its own `test` and `expect`. Left in scope, vitest tries to collect
 * them and fails at import time.
 *
 * This file replaces vite.config.ts for tests, so the `__APP_VERSION__`
 * define must be repeated here — both read the same package.json field.
 */
export default defineConfig({
  define: {
    __APP_VERSION__: JSON.stringify(version),
  },

  test: {
    environment: 'node',
    include: ['src/**/*.{test,spec}.{ts,tsx}'],
    exclude: ['e2e/**', 'node_modules/**', 'dist/**'],
  },
});
