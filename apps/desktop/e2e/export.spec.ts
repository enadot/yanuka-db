import { expect, test } from '@playwright/test';
import { parseCsv } from '@yanuka/utils';

/**
 * CSV export end-to-end in the browser build: the settings button walks every
 * contact through the repository, builds the file, and hands it over as a
 * download. The downloaded bytes must parse with our own parser and contain
 * the demo dataset — including the people the search e2e finds.
 */

test('exports every contact to a CSV the importer recognizes', async ({ page }) => {
  await page.goto('/#/settings');

  const downloadPromise = page.waitForEvent('download');
  await page.getByTestId('export-csv').click();
  const download = await downloadPromise;

  expect(download.suggestedFilename()).toMatch(/^contacts-\d{4}-\d{2}-\d{2}\.csv$/);
  const path = await download.path();
  const { readFile } = await import('node:fs/promises');
  const text = await readFile(path, 'utf-8');

  const { headers, rows } = parseCsv(text);
  expect(headers).toContain('שם מלא');
  expect(headers.some((header) => header.startsWith('טלפון'))).toBe(true);
  // The demo dataset ships 56 contacts; every one of them must be present.
  expect(rows.length).toBeGreaterThanOrEqual(56);
  expect(text).toContain('יעקב פרידמן');
  expect(text).toContain('ישראל סופר');
});

test('settings reports the security posture honestly in the browser build', async ({ page }) => {
  // In the browser there is no database file to encrypt; the card must say
  // that, not pretend. The encrypted/recovery-key variants are exercised by
  // the Rust tests — this asserts the state routing reaches the screen.
  await page.goto('/#/settings');
  await expect(page.getByTestId('security-state')).toContainText('באפליקציית המחשב');
});
