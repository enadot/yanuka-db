import { expect, test } from '@playwright/test';

/**
 * Sync between devices (ADR-039) through the real screens, on the browser
 * build's pretend pairing and its one shipped conflict: the unpaired state,
 * pairing from settings, a code for the next device, and a conflict settled
 * in favour of the other device.
 */

test('an unpaired machine says so, and pairs from settings', async ({ page }) => {
  await page.goto('/#/settings');
  await expect(page.getByTestId('sync-line')).toContainText('לא הוגדר שרת');

  const card = page.getByTestId('sync-card');
  await expect(card).toContainText('סנכרון בין מכשירים');
  await expect(page.getByTestId('sync-connect')).toBeDisabled();

  await page.getByTestId('sync-url').fill('192.168.1.10:8787');
  await page.getByTestId('sync-code').fill('K7PT-4MXQ');
  await page.getByTestId('sync-device-name').fill('המחשב במשרד');
  await page.getByTestId('sync-connect').click();

  await expect(page.getByTestId('sync-server-name')).toHaveText('שרת הדגמה');
  await expect(card).toContainText('המחשב במשרד');
  await expect(page.getByTestId('sync-line')).toContainText('מסונכרן');

  // A paired device can admit the next one.
  await page.getByTestId('sync-pair-code').click();
  await expect(page.getByTestId('pair-code')).toContainText('DEMO-CODE');

  // Leaving keeps the data and brings the form back.
  await page.getByTestId('sync-disconnect').click();
  await page.getByTestId('sync-disconnect-confirm').click();
  await expect(page.getByTestId('sync-url')).toBeVisible();
  await expect(page.getByTestId('sync-line')).toContainText('לא הוגדר שרת');
});

test('a conflict shows both versions and is settled by a person', async ({ page }) => {
  await page.goto('/');
  const link = page.getByTestId('sync-conflicts-link');
  await expect(link).toContainText('התנגשויות לטיפול');
  await link.click();

  const row = page.getByTestId('conflict-row').filter({ hasText: 'ישראל סופר' });
  await expect(row).toBeVisible();
  await expect(row).toContainText('טלפונים');
  await expect(row.getByTestId('conflict-remote-value')).toContainText('052-9990001');
  await expect(row).toContainText('המחשב הנייד');

  await row.getByTestId('conflict-remote').click();
  await expect(page.getByText('אין התנגשויות')).toBeVisible();
  await expect(page.getByTestId('sync-conflicts-link')).toHaveCount(0);

  // The chosen value is now on the card. Navigate inside the SPA: a reload
  // would reseed the in-memory repository and undo the decision.
  await page.getByRole('link', { name: 'חיפוש' }).click();
  await page.getByRole('textbox', { name: 'חיפוש אנשי קשר' }).fill('ישראל סופר');
  await page.getByRole('link', { name: /ישראל סופר/ }).first().click();
  await expect(page.getByRole('heading', { name: /ישראל סופר/ })).toBeVisible();
  await expect(page.getByText('052-9990001')).toBeVisible();
});
