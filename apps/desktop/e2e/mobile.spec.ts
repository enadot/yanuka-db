import { expect, test } from '@playwright/test';

/**
 * The phone layout (ADR-040), on a phone-sized viewport: the bottom bar and
 * the floating add button replace the side rail, the contact list becomes
 * tappable rows, facets move into a drawer, and everything still works with
 * a thumb — same screens, same data, same search.
 */

test('a phone gets a bottom bar, and search still finds the scribe', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByTestId('mobile-layout')).toBeVisible();
  await expect(page.getByTestId('mobile-nav')).toBeVisible();
  await expect(page.getByTestId('side-rail')).toHaveCount(0);

  await page.getByRole('textbox', { name: 'חיפוש אנשי קשר' }).fill('סופר סתם ירושלים');
  await expect(page.getByRole('link', { name: /ישראל סופר/ })).toBeVisible();

  // Facets are a drawer, not a column.
  await page.getByTestId('mobile-facets').click();
  await expect(page.getByRole('dialog')).toContainText('צמצום תוצאות');
  await page.keyboard.press('Escape');

  await page
    .getByRole('link', { name: /ישראל סופר/ })
    .first()
    .click();
  await expect(page.getByRole('heading', { name: /ישראל סופר/ })).toBeVisible();
});

test('the contact list is rows, and the add button opens the form', async ({ page }) => {
  await page.goto('/#/contacts');
  await expect(page.getByTestId('contact-cards')).toBeVisible();
  await expect(page.getByTestId('contact-card-row').first()).toBeVisible();

  // Every nav target is at least 44px tall — a thumb, not a cursor.
  const box = await page.getByTestId('mobile-nav').getByRole('link').first().boundingBox();
  expect(box?.height ?? 0).toBeGreaterThanOrEqual(44);

  await page.getByTestId('mobile-new-contact').click();
  await expect(page.getByLabel('שם מלא *')).toBeVisible();
  // …and hides on the form, where it would cover the save button.
  await expect(page.getByTestId('mobile-new-contact')).toHaveCount(0);
});

test('settings reach the notebooks and the sync card on a phone', async ({ page }) => {
  await page.goto('/#/settings');
  await expect(page.getByTestId('sync-card')).toBeVisible();
  await page.getByRole('link', { name: 'מחברות' }).click();
  await expect(page.getByRole('heading', { name: /מחברות/ })).toBeVisible();
});

test('the add button saves a contact with no network', async ({ page, context }) => {
  // The offline guarantee on the phone layout (ADR-041); the desktop layout is
  // covered in offline.spec.ts. Offline only after the app has mounted.
  await page.goto('/');
  await expect(page.getByTestId('mobile-new-contact')).toBeVisible();
  await context.setOffline(true);
  await expect.poll(() => page.evaluate(() => navigator.onLine)).toBe(false);

  await page.getByTestId('mobile-new-contact').click();
  await page.getByLabel('שם מלא *').fill('לאה בלי רשת');
  await page.getByRole('button', { name: 'הוספת איש קשר' }).click();
  await expect(page.getByRole('heading', { name: /לאה בלי רשת/ })).toBeVisible();

  await context.setOffline(false);
});
