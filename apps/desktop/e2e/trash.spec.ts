import { expect, test } from '@playwright/test';

/**
 * The recycle bin.
 *
 * Deletion was always soft, and the toast said so — but every list and search
 * filters deleted rows out, so once the toast faded the record was unreachable.
 * This pins the loop the product's first priority depends on: delete, walk away,
 * find it again, restore it whole.
 *
 * One SPA session throughout: a reload reseeds the in-memory repository.
 */

test('a deleted contact waits in the bin and comes back whole', async ({ page }) => {
  await page.goto('/#/contacts/new');
  await page.getByLabel('שם מלא *').fill('מנדל הנמחק');
  await page.getByLabel('עיר').fill('אנטוורפן');
  await page.getByRole('button', { name: 'הוספת מספר' }).click();
  await page.getByRole('textbox', { name: 'מספר' }).fill('03-6663333');
  await page.getByRole('button', { name: 'הוספת איש קשר' }).click();
  await expect(page.getByRole('heading', { name: /מנדל הנמחק/ })).toBeVisible();

  await page.getByLabel('הערה חדשה').fill('מכיר את כל הסוחרים ברחוב');
  await page.getByRole('button', { name: 'הוספת הערה' }).click();
  await expect(page.getByText('מכיר את כל הסוחרים ברחוב')).toBeVisible();

  await page.getByRole('button', { name: 'מחיקה' }).click();
  await page.getByRole('alertdialog').getByRole('button', { name: 'מחיקה' }).click();

  // Gone from search — which is exactly why the bin has to exist.
  await page.getByRole('navigation').getByRole('link', { name: 'חיפוש' }).click();
  await page.getByRole('textbox', { name: 'חיפוש אנשי קשר' }).fill('מנדל הנמחק');
  await expect(page.getByRole('link', { name: /מנדל הנמחק/ })).toHaveCount(0);

  // Reachable from settings long after the undo toast is gone.
  await page.getByRole('navigation').getByRole('link', { name: 'הגדרות' }).click();
  await page.getByRole('link', { name: 'פתיחת סל המחזור' }).click();

  const row = page.getByTestId('deleted-contact').filter({ hasText: 'מנדל הנמחק' });
  await expect(row).toBeVisible();
  // The deletion date badge — the contact's own name also contains "נמחק".
  await expect(row.getByText(/^נמחק \d/)).toBeVisible();

  await row.getByRole('button', { name: 'שחזור' }).click();
  await expect(page.getByText('סל המחזור ריק')).toBeVisible();

  // Restored whole: searchable again, with the phone and the note intact.
  await page.getByRole('navigation').getByRole('link', { name: 'חיפוש' }).click();
  await page.getByRole('textbox', { name: 'חיפוש אנשי קשר' }).fill('מנדל הנמחק');
  await page
    .getByRole('link', { name: /מנדל הנמחק/ })
    .first()
    .click();
  await expect(page.getByText('03-6663333')).toBeVisible();
  await expect(page.getByText('מכיר את כל הסוחרים ברחוב')).toBeVisible();
});
