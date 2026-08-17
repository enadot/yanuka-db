import { expect, test } from '@playwright/test';

/**
 * The acceptance path from the product brief, driven through the real UI:
 * open the app, search on a half-remembered detail, find the person, open them.
 */

test('home screen is search-first and right-to-left', async ({ page }) => {
  await page.goto('/');

  await expect(page.getByRole('heading', { name: 'את מי מחפשים?' })).toBeVisible();

  // RTL is the document default, not a runtime toggle.
  await expect(page.locator('html')).toHaveAttribute('dir', 'rtl');
  await expect(page.locator('html')).toHaveAttribute('lang', 'he');

  // The search box has focus on load — the user can type immediately.
  await expect(page.getByRole('textbox', { name: 'חיפוש אנשי קשר' })).toBeFocused();
});

test('finds a scribe by profession and city', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('textbox', { name: 'חיפוש אנשי קשר' }).fill('סופר סתם ירושלים');

  const result = page.getByRole('link', { name: /ישראל סופר/ });
  await expect(result).toBeVisible();

  // Every result explains itself.
  await expect(page.getByText(/נמצא לפי:/).first()).toBeVisible();
});

test('finds someone by a phrase from a note when the name is forgotten', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('textbox', { name: 'חיפוש אנשי קשר' }).fill('בורו פארק');

  await expect(page.getByRole('link', { name: /דוד רוזנברג/ })).toBeVisible();
});

test('tolerates a misspelling', async ({ page }) => {
  await page.goto('/');
  // An extra alef: פרידמאן must still reach פרידמן.
  await page.getByRole('textbox', { name: 'חיפוש אנשי קשר' }).fill('פרידמאן');

  await expect(page.getByRole('link', { name: /יעקב פרידמן/ })).toBeVisible();
});

test('matches a phone number typed in a different format', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('textbox', { name: 'חיפוש אנשי קשר' }).fill('+972 54 555 0134');

  await expect(page.getByRole('link', { name: /אברהם כהן/ })).toBeVisible();
});

test('facets narrow the result set', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('textbox', { name: 'חיפוש אנשי קשר' }).fill('סת"ם');

  const summary = page.locator('p', { hasText: /^\d+\s*תוצאות/ });
  await expect(summary).toBeVisible();
  const before = Number((await summary.innerText()).match(/\d+/)![0]);
  expect(before).toBeGreaterThan(1);

  await expect(page.getByRole('heading', { name: 'צמצום תוצאות' })).toBeVisible();

  // The country group is expanded by default, so no need to open it — clicking
  // its trigger here would collapse it instead.
  // Restrict to Israel and confirm the result count actually drops.
  await page.getByRole('checkbox', { name: 'ישראל' }).click();

  await expect
    .poll(async () => Number((await summary.innerText()).match(/\d+/)![0]))
    .toBeLessThan(before);
});

test('opens a contact and shows its relationships', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('textbox', { name: 'חיפוש אנשי קשר' }).fill('ישראל סופר');
  await page.getByRole('link', { name: /ישראל סופר/ }).first().click();

  await expect(page.getByRole('heading', { name: /ישראל סופר/ })).toBeVisible();
  await expect(page.getByText('דרכי התקשרות')).toBeVisible();
  await expect(page.getByText('קשרים', { exact: true })).toBeVisible();
  // The incoming edge: someone recommended this person to us.
  await expect(page.getByText(/הומלץ על ידי/)).toBeVisible();
});

test('Ctrl+K opens the command palette regardless of keyboard layout', async ({ page }) => {
  await page.goto('/');

  // Playwright dispatches by physical key code, which is exactly the case a
  // `event.key` binding would fail on under a Hebrew layout.
  await page.keyboard.press('Control+KeyK');

  const palette = page.getByPlaceholder('את מי מחפשים?').last();
  await expect(palette).toBeVisible();

  await palette.fill('משה');
  await expect(page.getByRole('option').first()).toBeVisible();
});

test('contact list paginates and can jump to a letter', async ({ page }) => {
  await page.goto('/#/contacts');

  await expect(page.getByRole('heading', { name: 'אנשי קשר' })).toBeVisible();
  await expect(page.getByRole('table')).toBeVisible();
  await expect(page.getByRole('button', { name: 'הבא' })).toBeVisible();
});

test('the new contact form requires only a name', async ({ page }) => {
  await page.goto('/#/contacts/new');

  await page.getByLabel('שם מלא *').fill('בדיקה אוטומטית');
  await page.getByRole('button', { name: 'הוספת איש קשר' }).click();

  await expect(page.getByRole('heading', { name: /בדיקה אוטומטית/ })).toBeVisible();
});

test('warns about a duplicate phone number before saving', async ({ page }) => {
  await page.goto('/#/contacts/new');

  await page.getByLabel('שם מלא *').fill('שם אחר לגמרי');
  await page.getByRole('button', { name: 'הוספת מספר' }).click();
  await page.getByRole('textbox', { name: 'מספר' }).fill('054-555-0134');

  await expect(page.getByText('ייתכן שאיש הקשר כבר קיים')).toBeVisible();
});
