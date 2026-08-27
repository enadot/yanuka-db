import { expect, test } from '@playwright/test';

/**
 * The recovery net, end to end: history that remembers a field's previous
 * value, and a trash that gives a deleted contact a durable way back — all
 * inside one SPA session, since a reload reseeds the in-memory repository.
 */

test('an edit is remembered in history, and a deleted contact returns from the trash', async ({
  page,
}) => {
  // A fresh contact keeps the flow independent of the demo data.
  await page.goto('/#/contacts/new');
  await page.getByLabel('שם מלא *').fill('יונה הנבדק');
  await page.getByLabel('עיר').fill('ירושלים');
  await page.getByRole('button', { name: 'הוספת איש קשר' }).click();
  await expect(page.getByRole('heading', { name: /יונה הנבדק/ })).toBeVisible();

  // Creation is already on the record.
  await expect(page.getByTestId('history-entry').filter({ hasText: 'הרשומה נוצרה' })).toBeVisible();

  // Change the city; history shows the field, what it was, and what it is now.
  await page.getByRole('link', { name: 'עריכה' }).click();
  await page.getByLabel('עיר').fill('צפת');
  await page.getByRole('button', { name: 'שמירת שינויים' }).click();
  await expect(page.getByRole('heading', { name: /יונה הנבדק/ })).toBeVisible();
  const update = page.getByTestId('history-entry').filter({ hasText: 'עודכן' });
  await expect(update).toContainText('עיר');
  await expect(update).toContainText('ירושלים');
  await expect(update).toContainText('צפת');

  // Delete, then walk in through the durable path: the trash screen.
  await page.getByRole('button', { name: 'מחיקה' }).click();
  await page.getByRole('alertdialog').getByRole('button', { name: 'מחיקה' }).click();
  await expect(page.getByRole('heading', { name: 'אנשי קשר' })).toBeVisible();

  await page.getByRole('link', { name: 'סל המחזור' }).click();
  const row = page.getByTestId('trash-row').filter({ hasText: 'יונה הנבדק' });
  await expect(row).toBeVisible();
  await expect(row).toContainText('נמחק:');

  await row.getByTestId('restore-contact').click();
  await expect(page.getByTestId('trash-row').filter({ hasText: 'יונה הנבדק' })).toHaveCount(0);

  // Back among the living, with the round trip on the record.
  await page.getByRole('navigation').getByRole('link', { name: 'אנשי קשר' }).click();
  await page.getByRole('link', { name: /יונה הנבדק/ }).first().click();
  await expect(page.getByRole('heading', { name: /יונה הנבדק/ })).toBeVisible();
  await expect(
    page.getByTestId('history-entry').filter({ hasText: 'שוחזר מסל המחזור' }),
  ).toBeVisible();
});
