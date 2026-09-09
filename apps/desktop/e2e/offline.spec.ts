import { expect, test } from '@playwright/test';
import { goOffline } from './go-offline';

/**
 * The offline guarantee (docs/PRODUCT.md, promise 1): everything local works
 * with no network — adding, editing, annotating, finding.
 *
 * The data source here is the in-memory repository and, in the shipped app,
 * SQLite over Tauri IPC; neither touches the network. What can break offline
 * is the frontend's own plumbing, and once did: the data layer paused every
 * save until the window reported a connection (ADR-041). So this goes offline
 * AFTER the app has mounted — the data layer learns about connectivity from
 * the window's events, and the page itself still has to be served — and then
 * stays inside the SPA, since a page load would need the server.
 */

test('a contact is added, edited, annotated and found with no network', async ({
  page,
  context,
}) => {
  await page.goto('/');
  await expect(page.getByRole('textbox', { name: 'חיפוש אנשי קשר' })).toBeVisible();
  await goOffline(page, context);

  // Add.
  await page.getByTestId('side-rail').getByRole('button', { name: 'איש קשר חדש' }).click();
  await page.getByLabel('שם מלא *').fill('רחל בלי רשת');
  await page.getByRole('textbox', { name: 'עיר' }).fill('צפת');
  await page.getByRole('button', { name: 'הוספת איש קשר' }).click();
  await expect(page.getByRole('heading', { name: /רחל בלי רשת/ })).toBeVisible();

  // Edit.
  await page.getByRole('link', { name: 'עריכה' }).click();
  await page.getByRole('textbox', { name: 'מקצוע' }).fill('מורה');
  await page.getByRole('button', { name: 'שמירת שינויים' }).click();
  await expect(page.getByRole('heading', { name: /רחל בלי רשת/ })).toBeVisible();
  await expect(page.getByText('מורה').first()).toBeVisible();

  // Annotate.
  await page.getByTestId('new-note-body').fill('נרשמה בערב בלי אינטרנט');
  await page.getByTestId('add-note').click();
  await expect(
    page.getByTestId('contact-note').filter({ hasText: 'בלי אינטרנט' }),
  ).toBeVisible();

  // Find — by the city typed a moment ago, and by the note.
  await page.getByRole('navigation').getByRole('link', { name: 'חיפוש' }).click();
  await page.getByRole('textbox', { name: 'חיפוש אנשי קשר' }).fill('צפת');
  await expect(page.getByRole('link', { name: /רחל בלי רשת/ }).first()).toBeVisible();
  await page.getByRole('textbox', { name: 'חיפוש אנשי קשר' }).fill('בלי אינטרנט');
  await expect(page.getByRole('link', { name: /רחל בלי רשת/ }).first()).toBeVisible();

  // All of it happened with the browser still reporting no network.
  expect(await page.evaluate(() => navigator.onLine)).toBe(false);
  await context.setOffline(false);
});

test('a contact opened for the first time while offline is shown, not "not found"', async ({
  page,
  context,
}) => {
  await page.goto('/');
  await expect(page.getByRole('textbox', { name: 'חיפוש אנשי קשר' })).toBeVisible();
  await goOffline(page, context);

  // A card never visited in this session: its data has to be read now, with
  // the window offline. A paused read would fall through to "not found".
  await page.getByRole('textbox', { name: 'חיפוש אנשי קשר' }).fill('בורו פארק');
  await page.getByRole('link', { name: /דוד רוזנברג/ }).first().click();
  await expect(page.getByRole('heading', { name: /דוד רוזנברג/ })).toBeVisible();
  await expect(page.getByText('איש הקשר לא נמצא')).toHaveCount(0);

  await context.setOffline(false);
});
