import { defineConfig } from 'vitest/config';

/**
 * Unit-test configuration for the desktop app.
 *
 * `e2e/` is excluded because those specs are driven by Playwright, which
 * provides its own `test` and `expect`. Left in scope, vitest tries to collect
 * them and fails at import time.
 */
export default defineConfig({
  test: {
    environment: 'node',
    include: ['src/**/*.{test,spec}.{ts,tsx}'],
    exclude: ['e2e/**', 'node_modules/**', 'dist/**'],
  },
});
