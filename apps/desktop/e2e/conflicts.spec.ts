import { expect, test } from '@playwright/test';

/**
 * The conflict screen reaches the browser build without a local database.
 *
 * Its interesting behaviour — two answers side by side, nothing preselected,
 * a partial decision — needs conflicts to exist, which needs sync, which needs
 * a Tauri shell; that is covered by the component suite and, end to end, in
 * Rust. What only a real browser can catch is this screen throwing on a
 * desktop API that is not there, which would take the route down entirely.
 */
test('the conflict screen loads and says there is nothing to decide', async ({ page }) => {
  await page.goto('/#/conflicts');

  await expect(page.getByRole('heading', { name: 'שינויים בשתי גרסאות' })).toBeVisible();
  await expect(page.getByText('אין מה להכריע')).toBeVisible();

  await page.getByRole('link', { name: 'חזרה להגדרות' }).click();
  await expect(page.getByRole('heading', { name: 'הגדרות' })).toBeVisible();
});
