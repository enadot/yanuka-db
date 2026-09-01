import { expect, test } from '@playwright/test';

/**
 * The graph, written from the UI.
 *
 * The product's defining question — "who was that Jew from London the rabbi
 * recommended?" — is a question about an edge and about a sentence somebody
 * wrote down. Both must be enterable from the card, and both must come back
 * through search afterwards, or the archive can only answer questions about
 * data that arrived with the demo seed.
 *
 * One SPA session throughout: a reload reseeds the in-memory repository.
 */

test('records who recommended whom and finds the pair from either end', async ({ page }) => {
  await page.goto('/#/contacts/new');
  await page.getByLabel('שם מלא *').fill('הרב שמעון הממליץ');
  await page.getByRole('button', { name: 'הוספת איש קשר' }).click();
  await expect(page.getByRole('heading', { name: /הרב שמעון הממליץ/ })).toBeVisible();

  await page.getByRole('navigation').getByRole('button', { name: 'איש קשר חדש' }).click();
  await page.getByLabel('שם מלא *').fill('נחום המומלץ מלונדון');
  await page.getByRole('button', { name: 'הוספת איש קשר' }).click();
  await expect(page.getByRole('heading', { name: /נחום המומלץ מלונדון/ })).toBeVisible();

  // From the recommended contact's card, record who recommended him.
  await page.getByRole('button', { name: 'הוספת קשר' }).click();
  await page.getByLabel('סוג הקשר').click();
  await page.getByRole('option', { name: 'הומלץ על ידי', exact: true }).click();
  await page.getByLabel('הערה על הקשר').fill('הכיר לנו אותו בכינוס');
  await page.getByLabel('חיפוש איש קשר לקישור').fill('שמעון');
  await page.getByRole('button', { name: /הרב שמעון הממליץ/ }).click();

  await expect(page.getByText('הכיר לנו אותו בכינוס')).toBeVisible();
  await expect(page.getByText('הומלץ על ידי', { exact: true })).toBeVisible();

  // And the same edge reads correctly from the other end.
  await page.getByRole('link', { name: 'הרב שמעון הממליץ' }).click();
  await expect(page.getByRole('heading', { name: /הרב שמעון הממליץ/ })).toBeVisible();
  await expect(page.getByRole('link', { name: 'נחום המומלץ מלונדון' })).toBeVisible();
});

test('a note written on the card is findable by a word from it', async ({ page }) => {
  await page.goto('/#/contacts/new');
  await page.getByLabel('שם מלא *').fill('בעל יומן ההערות');
  await page.getByRole('button', { name: 'הוספת איש קשר' }).click();
  await expect(page.getByRole('heading', { name: /בעל יומן ההערות/ })).toBeVisible();

  await page.getByLabel('הערה חדשה').fill('אפשר להיעזר בו בנושא בתי כנסת באנטוורפן');
  await page.getByRole('button', { name: 'הוספת הערה' }).click();
  await expect(page.getByText('בתי כנסת באנטוורפן')).toBeVisible();

  await page.getByRole('navigation').getByRole('link', { name: 'חיפוש' }).click();
  await page.getByRole('textbox', { name: 'חיפוש אנשי קשר' }).fill('אנטוורפן');
  await expect(page.getByRole('link', { name: /בעל יומן ההערות/ })).toBeVisible();
});

test('an edit does not delete the details the form was not showing', async ({ page }) => {
  // The regression this guards: a save replaces the child collections
  // wholesale, so anything the form fails to carry back it deletes.
  await page.goto('/#/contacts/new');
  await page.getByLabel('שם מלא *').fill('יעקב פרידמן');
  await page.getByRole('button', { name: 'הוספת כתובת' }).click();
  await page.getByPlaceholder('name@example.com').fill('friedman@example.com');
  await page.getByRole('button', { name: 'הוספת שם' }).click();
  await page.getByPlaceholder('Friedman / פרידמאן').fill('Friedman');
  await page.getByRole('button', { name: 'הוספת איש קשר' }).click();
  await expect(page.getByRole('heading', { name: /יעקב פרידמן/ })).toBeVisible();
  await expect(page.getByText('friedman@example.com')).toBeVisible();

  // Edit something unrelated and save.
  await page.getByRole('link', { name: 'עריכה' }).click();
  await page.getByLabel('עיר').fill('בני ברק');
  await page.getByRole('button', { name: 'שמירת שינויים' }).click();

  await expect(page.getByRole('heading', { name: /יעקב פרידמן/ })).toBeVisible();
  await expect(page.getByText('friedman@example.com')).toBeVisible();
  await expect(page.getByText('Friedman', { exact: true })).toBeVisible();
});

test('attaches a contact to an institution, creating it inline', async ({ page }) => {
  await page.goto('/#/contacts/new');
  await page.getByLabel('שם מלא *').fill('ראש הישיבה החדש');
  await page.getByLabel('חיפוש מוסד').fill('ישיבת בדיקה');
  await page.getByRole('button', { name: /הוספת מוסד חדש בשם/ }).click();
  await page.getByLabel('תפקיד בישיבת בדיקה').fill('ראש ישיבה');
  await page.getByRole('button', { name: 'הוספת איש קשר' }).click();

  await expect(page.getByRole('heading', { name: /ראש הישיבה החדש/ })).toBeVisible();
  // Scoped to the card, because the inline-create toast quotes the name too.
  const organizations = page.getByRole('main').getByText('ישיבת בדיקה', { exact: true });
  await expect(organizations).toBeVisible();
  await expect(page.getByRole('main').getByText('ראש ישיבה', { exact: true })).toBeVisible();
});
