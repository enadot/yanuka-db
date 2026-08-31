import { expect, test } from '@playwright/test';

/**
 * The notebook workbench, on the browser build's demo page.
 *
 * The real pipeline — image segmentation and the writer memory — runs in Rust
 * and is covered by `crates/yanuka-db/tests/ocr.rs`. What this asserts is the
 * user-facing loop: the screen renders the page's word boxes, a correction
 * sticks, and the "learned twin" behavior is visible in the UI.
 */
test('a notebook correction teaches the demo twin in the workbench', async ({ page }) => {
  await page.goto('/#/notebooks');
  await expect(page.getByTestId('notebook-page-card')).toBeVisible();

  await page.getByTestId('notebook-page-link').click();
  await expect(page.getByTestId('token-input-demo-1')).toBeVisible();

  // Correct the first word; the demo's twin shape learns it.
  await page.getByTestId('token-input-demo-1').fill('אברהם');
  await page.getByTestId('token-input-demo-1').press('Enter');

  await expect(page.getByTestId('token-input-demo-3')).toHaveValue('אברהם');
  await expect(page.getByText('זוהה מהכתב', { exact: true })).toBeVisible();

  // The untaught word is untouched.
  await expect(page.getByTestId('token-input-demo-2')).toHaveValue('');
});
