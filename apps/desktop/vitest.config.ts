import { defineConfig } from 'vitest/config';

/**
 * Unit-test configuration for the desktop app.
 *
 * `e2e/` is excluded because those specs are driven by Playwright, which
 * provides its own `test` and `expect`. Left in scope, vitest tries to collect
 * them and fails at import time.
 *
 * The environment stays `node`, and the two suites that render a component opt
 * into jsdom with a `@vitest-environment` docblock of their own. Making jsdom
 * the default instead looks tidier and quietly breaks the IPC parity test,
 * which reads Rust source through `import.meta.url` — a file URL under node and
 * an http one under jsdom.
 */
export default defineConfig({
  test: {
    environment: 'node',
    include: ['src/**/*.{test,spec}.{ts,tsx}'],
    exclude: ['e2e/**', 'node_modules/**', 'dist/**'],
  },
});
