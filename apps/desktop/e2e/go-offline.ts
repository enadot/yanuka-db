import { expect, type BrowserContext, type Page } from '@playwright/test';

/**
 * Take the browser offline the way a laptop does — after the app has mounted.
 *
 * The data layer learns about connectivity from the window's own events, so
 * going offline before the page is served would test nothing (and the page
 * could not load). Waits until the window itself reports the change, which
 * is what the data layer sees.
 */
export async function goOffline(page: Page, context: BrowserContext) {
  await context.setOffline(true);
  await expect.poll(() => page.evaluate(() => navigator.onLine)).toBe(false);
}
