import { expect, test } from '@playwright/test';

/**
 * The duplicate-resolution flow end-to-end, inside one SPA session (a reload
 * would reseed the in-memory repository): create the same person twice with
 * the same number in two formats, find the pair on the duplicates screen,
 * merge, and verify nothing was lost.
 */

test('finds a duplicate created twice and merges it without losing data', async ({ page }) => {
  await page.goto('/#/contacts/new');
  await page.getByLabel('שם מלא *').fill('זושא מרגלית');
  await page.getByRole('button', { name: 'הוספת מספר' }).click();
  await page.getByRole('textbox', { name: 'מספר' }).fill('054-777-2001');
  await page.getByRole('button', { name: 'הוספת איש קשר' }).click();
  await expect(page.getByRole('heading', { name: /זושא מרגלית/ })).toBeVisible();

  await page.getByRole('navigation').getByRole('button', { name: 'איש קשר חדש' }).click();
  await page.getByLabel('שם מלא *').fill('ר׳ זושא מרגלית');
  await page.getByRole('button', { name: 'הוספת מספר' }).click();
  await page.getByRole('textbox', { name: 'מספר' }).fill('+972547772001');
  await page.getByRole('button', { name: 'הוספת איש קשר' }).click();
  await expect(page.getByRole('heading', { name: /ר׳ זושא מרגלית/ })).toBeVisible();

  await page.getByRole('navigation').getByRole('link', { name: 'הגדרות' }).click();
  await page.getByRole('link', { name: 'איתור כפילויות' }).click();

  const pair = page
    .getByTestId('duplicate-pair')
    .filter({ hasText: 'זושא מרגלית' })
    .first();
  await expect(pair).toBeVisible();
  await expect(pair.getByText('אותו מספר טלפון')).toBeVisible();

  // Keep the side titled with the honorific; merge the plain one into it.
  await pair.getByRole('button', { name: 'לשמור את ר׳ זושא מרגלית' }).click();
  await page.getByTestId('confirm-merge').click();

  await expect(page.getByText(/מוזג אל/)).toBeVisible();

  // The kept contact holds both phone formats; the merged one is gone.
  await page.getByRole('navigation').getByRole('link', { name: 'חיפוש' }).click();
  await page.getByRole('textbox', { name: 'חיפוש אנשי קשר' }).fill('זושא');
  const results = page.getByRole('link', { name: /זושא מרגלית/ });
  await expect(results).toHaveCount(1);
  await results.click();
  await expect(page.getByText('054-777-2001')).toBeVisible();
  await expect(page.getByText('+972547772001')).toBeVisible();
});
