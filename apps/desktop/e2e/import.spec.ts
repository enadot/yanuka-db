import { expect, test } from '@playwright/test';

/**
 * The CSV import flow end-to-end: pick a file, see the auto-detected mapping,
 * import, read the summary, find the imported person by search.
 *
 * The file is Hebrew with a quoted comma inside a name and one row with no
 * name — the summary must report exactly that row and import the rest.
 */

const CSV = [
  'שם,טלפון נייד,עיר,מקצוע,הערות',
  'שמעון ברגר,054-7770001,אנטוורפן,חלפן כספים,"הומלץ ע""י אדלר, לפגוש ביום שלישי"',
  ',054-7770002,לונדון,,שורה בלי שם',
  'אליהו ורטהיימר,054-7770003,ציריך,שוחט,',
].join('\n');

test('imports a Hebrew CSV, reports the nameless row and finds the rest', async ({ page }) => {
  await page.goto('/#/import');

  await page.getByTestId('import-file-input').setInputFiles({
    name: 'מחברת-ישנה.csv',
    mimeType: 'text/csv',
    buffer: Buffer.from(CSV, 'utf-8'),
  });

  // Auto-detection announced the file and its data rows.
  await expect(page.getByText('מחברת-ישנה.csv')).toBeVisible();
  await expect(page.getByText('3 שורות נתונים')).toBeVisible();

  // The nameless row is flagged before anything is written.
  await expect(page.getByText('1 שורות ללא שם ידווחו בסיכום')).toBeVisible();

  await page.getByTestId('run-import').click();

  const summary = page.getByTestId('import-summary');
  await expect(summary).toBeVisible();
  // "נוצרו 2 אנשי קשר … 1 שורות לא יובאו", and the failed row names its reason.
  await expect(summary.locator('strong').first()).toHaveText('2');
  await expect(summary.locator('strong').nth(1)).toHaveText('1');
  await expect(summary.getByText(/אין שם/)).toBeVisible();

  // The imported people are searchable like any hand-entered contact. Navigate
  // inside the SPA — a full reload would reseed the in-memory repository.
  await page.getByRole('navigation').getByRole('link', { name: 'חיפוש' }).click();
  await page.getByRole('textbox', { name: 'חיפוש אנשי קשר' }).fill('חלפן אנטוורפן');
  await expect(page.getByRole('link', { name: /שמעון ברגר/ })).toBeVisible();
});

test('the import screen is reachable from settings', async ({ page }) => {
  await page.goto('/#/settings');
  await page.getByRole('link', { name: 'ייבוא מקובץ' }).click();
  await expect(page.getByRole('heading', { name: 'ייבוא אנשי קשר מקובץ CSV' })).toBeVisible();
});
