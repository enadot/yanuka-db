import { defineConfig, devices } from '@playwright/test';

/**
 * End-to-end tests against the browser build.
 *
 * The Tauri shell cannot be built on Linux CI (it needs webkit2gtk), but the
 * frontend is the same bundle either way — only the repository behind it
 * differs. Running these against the in-memory repository therefore exercises
 * the real screens, the real search engine and the real demo data, which is the
 * bulk of what could break.
 */
export default defineConfig({
  testDir: './e2e',
  fullyParallel: true,
  forbidOnly: Boolean(process.env.CI),
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? 'list' : 'line',

  use: {
    baseURL: 'http://127.0.0.1:4173',
    locale: 'he-IL',
    trace: 'retain-on-failure',
  },

  projects: [
    {
      name: 'chromium',
      testIgnore: 'mobile.spec.ts',
      use: {
        ...devices['Desktop Chrome'],
        launchOptions: {
          // Provided by the environment image; never downloaded at test time.
          // Falls back to Playwright's own resolution when unset, which is what
          // GitHub Actions uses after `playwright install chromium`.
          executablePath: process.env.CHROMIUM_PATH || undefined,
        },
      },
    },
    {
      // The Android build runs this same bundle in a webview a third the width
      // of the desktop one. A phone-sized viewport is the cheapest way to keep
      // that honest on every commit, on a Linux runner with no emulator: it
      // catches a layout that overflows or a control a thumb cannot reach,
      // which is most of what breaks between the two.
      name: 'mobile',
      testMatch: 'mobile.spec.ts',
      use: {
        ...devices['Pixel 7'],
        launchOptions: {
          executablePath: process.env.CHROMIUM_PATH || undefined,
        },
      },
    },
  ],

  webServer: {
    command: 'pnpm preview --port 4173 --strictPort',
    url: 'http://127.0.0.1:4173',
    reuseExistingServer: !process.env.CI,
    timeout: 120_000,
  },
});
