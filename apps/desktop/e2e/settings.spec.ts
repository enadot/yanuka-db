import { expect, test } from '@playwright/test';

/**
 * The settings screen renders in the browser build.
 *
 * Thin on purpose. The sync card's real behaviour needs a Tauri shell and a
 * server, and is covered in Rust end to end; what this catches is the failure
 * that browser-only CI otherwise cannot see at all — a card that throws on a
 * missing desktop API and takes the whole screen down with it, including the
 * backup controls that have nothing to do with sync.
 */
test('settings renders, and says plainly that the demo build has nothing to sync', async ({
  page,
}) => {
  await page.goto('/#/settings');

  await expect(page.getByRole('heading', { name: 'הגדרות' })).toBeVisible();
  await expect(page.getByText('סנכרון בין מכשירים')).toBeVisible();
  await expect(page.getByText('במצב הדגמה אין מסד נתונים מקומי')).toBeVisible();

  // The rest of the screen survived it.
  await expect(page.getByRole('link', { name: 'פתיחת סל המחזור' })).toBeVisible();
  await expect(page.getByRole('button', { name: /ייצוא כל אנשי הקשר/ })).toBeVisible();
});
