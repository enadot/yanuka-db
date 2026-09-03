import { expect, test } from '@playwright/test';

/**
 * Smart categories (ADR-038) through the real UI, on the browser build's demo
 * data: tiles on the home screen, a shelf with its members, a new rule with a
 * live preview, and a hand override from the contact card.
 */

test('home screen offers category tiles that open the shelf', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByTestId('home-categories')).toBeVisible();

  const scribes = page.getByTestId('home-category-tile').filter({ hasText: 'סופרי סת"ם' });
  await expect(scribes).toBeVisible();
  await scribes.click();

  await expect(page.getByTestId('category-title')).toHaveText('סופרי סת"ם');
  await expect(page.getByTestId('category-member').filter({ hasText: 'ישראל סופר' })).toBeVisible();
  // Rule membership is explained on every row.
  await expect(page.getByText('לפי הכלל').first()).toBeVisible();
});

test('a new rule previews its members before it is saved', async ({ page }) => {
  await page.goto('/#/categories');
  await page.getByTestId('category-new').click();

  await page.getByTestId('category-name').fill('חשמלאים לבדיקה');
  await page.getByTestId('category-smart').click();
  await page.getByTestId('condition-value-0').fill('חשמלאי');

  // Two electricians in the demo data.
  await expect(page.getByTestId('category-preview')).toContainText(/[1-9]\d* אנשי קשר/);

  await page.getByTestId('category-save').click();
  const row = page.getByTestId('category-row').filter({ hasText: 'חשמלאים לבדיקה' });
  await expect(row).toBeVisible();
  await expect(row.getByText('חכמה')).toBeVisible();
});

test('a person can be taken off a shelf from the card, and put back', async ({ page }) => {
  await page.goto('/?q=ישראל סופר');
  await page.getByRole('link', { name: /ישראל סופר/ }).click();

  const pill = page.getByText('סופרי סת"ם', { exact: true });
  await expect(pill).toBeVisible();

  await page.getByTestId('contact-categories-menu').click();
  const box = page.getByRole('checkbox', { name: 'סופרי סת"ם' });
  await expect(box).toBeChecked();
  await box.click();
  await expect(box).not.toBeChecked();
  await page.keyboard.press('Escape');
  await expect(pill).toHaveCount(0);

  await page.getByTestId('contact-categories-menu').click();
  await page.getByRole('checkbox', { name: 'סופרי סת"ם' }).click();
  await page.keyboard.press('Escape');
  await expect(page.getByText('סופרי סת"ם', { exact: true })).toBeVisible();
});
