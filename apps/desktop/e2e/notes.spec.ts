import { expect, test } from '@playwright/test';

/**
 * Notes and relationships, written from the contact card — each flow inside
 * one SPA session, since a reload reseeds the in-memory repository.
 *
 * The note test is the product's core promise exercised end-to-end: a phrase
 * typed on the card right now is how the person will be found in fifteen
 * years, so it must be findable the moment it is saved.
 */

test('a note added on the card finds the contact by its phrase, then edits and deletes', async ({
  page,
}) => {
  await page.goto('/');
  await page.getByRole('textbox', { name: 'חיפוש אנשי קשר' }).fill('ישראל סופר');
  await page.getByRole('link', { name: /ישראל סופר/ }).first().click();
  await expect(page.getByRole('heading', { name: /ישראל סופר/ })).toBeVisible();

  await page.getByTestId('new-note-body').fill('פגשתי אותו בוועידה בהלסינקי');
  await page.getByTestId('add-note').click();
  const note = page.getByTestId('contact-note').filter({ hasText: 'בוועידה בהלסינקי' });
  await expect(note).toBeVisible();

  // The phrase finds the person from the home search — the reason notes exist.
  await page.getByRole('navigation').getByRole('link', { name: 'חיפוש' }).click();
  // `.first()`: the contact was just visited, so it can also sit in the
  // recent-contacts list — two links with the same accessible name.
  await page.getByRole('textbox', { name: 'חיפוש אנשי קשר' }).fill('הלסינקי');
  await expect(page.getByRole('link', { name: /ישראל סופר/ }).first()).toBeVisible();

  // Edit the note; the card shows the new wording.
  await page.getByRole('link', { name: /ישראל סופר/ }).first().click();
  await note.getByRole('button', { name: 'עריכת הערה' }).click();
  await page.getByTestId('edit-note-body').fill('עבר לגור בהמבורג');
  await page.getByTestId('save-note').click();
  const edited = page.getByTestId('contact-note').filter({ hasText: 'עבר לגור בהמבורג' });
  await expect(edited).toBeVisible();

  // Delete it, and it is gone from the card.
  await edited.getByRole('button', { name: 'מחיקת הערה' }).click();
  await page.getByTestId('confirm-delete-note').click();
  await expect(edited).toHaveCount(0);
});

test('a relationship recorded on one card reads correctly from both sides and can be removed', async ({
  page,
}) => {
  // A fresh contact keeps the demo dataset's own edges out of the assertions.
  await page.goto('/#/contacts/new');
  await page.getByLabel('שם מלא *').fill('שמעון בודק');
  await page.getByRole('button', { name: 'הוספת איש קשר' }).click();
  await expect(page.getByRole('heading', { name: /שמעון בודק/ })).toBeVisible();

  // Record: שמעון בודק המליץ על ישראל סופר.
  await page.getByTestId('add-relationship').click();
  await page.getByTestId('relationship-type').click();
  await page.getByRole('option', { name: 'המליץ על' }).click();
  await page.getByTestId('relationship-contact').fill('ישראל סופר');
  await page.getByTestId('relationship-suggestion').first().click();
  await expect(page.getByTestId('relationship-chosen')).toContainText('ישראל סופר');
  await page.getByTestId('save-relationship').click();

  const outgoing = page.getByTestId('relationship-row').filter({ hasText: 'ישראל סופר' });
  await expect(outgoing).toContainText('המליץ על');

  // The far side reads the same edge through the inverse label.
  await outgoing.getByRole('link', { name: /ישראל סופר/ }).click();
  await expect(page.getByRole('heading', { name: /ישראל סופר/ })).toBeVisible();
  const incoming = page.getByTestId('relationship-row').filter({ hasText: 'שמעון בודק' });
  await expect(incoming).toContainText('הומלץ על ידי');

  // Deleting from either side removes the single stored edge everywhere.
  await incoming.getByRole('button', { name: 'מחיקת הקשר עם שמעון בודק' }).click();
  await page.getByTestId('confirm-delete-relationship').click();
  await expect(incoming).toHaveCount(0);

  await page.getByRole('navigation').getByRole('link', { name: 'חיפוש' }).click();
  await page.getByRole('textbox', { name: 'חיפוש אנשי קשר' }).fill('שמעון בודק');
  await page.getByRole('link', { name: /שמעון בודק/ }).first().click();
  await expect(page.getByTestId('relationship-row')).toHaveCount(0);
  await expect(page.getByText('עוד לא נרשם כאן מי מכיר את מי', { exact: false })).toBeVisible();
});
