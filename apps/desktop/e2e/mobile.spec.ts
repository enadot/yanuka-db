import { expect, test } from '@playwright/test';

/**
 * The same application, on a phone.
 *
 * Android runs this exact bundle in a webview about a third the width of the
 * desktop window, so what needs guarding is not "does it render" but the two
 * things that break silently at that size: content wider than the screen, which
 * turns every screen into a horizontal scroll, and controls too small or too
 * high to reach with a thumb.
 *
 * Runs only in the `mobile` project — see playwright.config.ts.
 */

/**
 * Nothing may stick out past the page.
 *
 * Measured element-by-element against the body's own box rather than by
 * comparing `scrollWidth` to `clientWidth`, which is the obvious way and is
 * wrong here: under mobile emulation `documentElement.scrollWidth` reports the
 * layout viewport including the scrollbar gutter, so it reads as 20px of
 * overflow on every page including an empty one. Comparing boxes to boxes also
 * survives the viewport being scrolled, and names the culprit when it fails.
 */
async function assertNothingSticksOut(page: import('@playwright/test').Page) {
  const offenders = await page.evaluate(() => {
    const page = document.body.getBoundingClientRect();
    const out: string[] = [];
    for (const element of document.querySelectorAll('body *')) {
      const box = element.getBoundingClientRect();
      if (box.width === 0 || box.height === 0) continue;
      // `sr-only` content is clipped to a 1px box by design.
      if (element.classList.contains('sr-only')) continue;
      if (box.left < page.left - 1 || box.right > page.right + 1) {
        out.push(
          `<${element.tagName.toLowerCase()} class="${String(element.className).slice(0, 60)}">`,
        );
      }
    }
    return out.slice(0, 5);
  });
  expect(offenders, 'these reach past the edge of the screen').toEqual([]);
}

test('navigation sits at the bottom, where a thumb is', async ({ page }) => {
  await page.goto('/');

  const bar = page.getByRole('navigation', { name: 'ניווט' });
  await expect(bar).toBeVisible();

  const box = (await bar.boundingBox())!;
  const viewport = page.viewportSize()!;
  expect(box.y, 'the bar is not in the lower half of the screen').toBeGreaterThan(
    viewport.height / 2,
  );

  // And it does not cover the content: the bar begins where the scrollable
  // area ends, rather than floating over the last row of results.
  const main = (await page.locator('main').boundingBox())!;
  expect(main.y + main.height).toBeLessThanOrEqual(box.y + 1);
});

test('every navigation target is big enough to hit', async ({ page }) => {
  await page.goto('/');

  const items = page.getByRole('navigation', { name: 'ניווט' }).locator('a, button');
  await expect(items).toHaveCount(4);

  for (const item of await items.all()) {
    const box = (await item.boundingBox())!;
    // 44px is the smallest target Apple and Google both consider reachable;
    // this bar is built at 64.
    expect(box.height, `${await item.textContent()} is too short`).toBeGreaterThanOrEqual(44);
    expect(box.width, `${await item.textContent()} is too narrow`).toBeGreaterThanOrEqual(44);
  }
});

test('the desktop rail is not merely hidden behind the content', async ({ page }) => {
  await page.goto('/');
  // Two navigations rendered at once would mean the rail is still taking part
  // in the layout — and its 15rem would be coming out of the results.
  await expect(page.getByRole('navigation')).toHaveCount(1);
  await assertNothingSticksOut(page);
});

test('searching, opening a contact and going back all work at phone width', async ({ page }) => {
  await page.goto('/');

  await page.getByRole('textbox', { name: 'חיפוש אנשי קשר' }).fill('סופר סתם ירושלים');
  const result = page.getByRole('link', { name: /ישראל סופר/ });
  await expect(result).toBeVisible();
  await assertNothingSticksOut(page);

  await result.click();
  await expect(page.getByRole('heading', { name: /ישראל סופר/ })).toBeVisible();
  await assertNothingSticksOut(page);

  await page.getByRole('link', { name: 'אנשי קשר' }).click();
  await expect(page).toHaveURL(/#\/contacts/);
  await assertNothingSticksOut(page);
});

test('the long screens stay inside the viewport', async ({ page }) => {
  for (const path of [
    '/#/contacts',
    '/#/settings',
    '/#/contacts/new',
    '/#/conflicts',
    '/#/trash',
    '/#/duplicates',
    '/#/import',
  ]) {
    await page.goto(path);
    await expect(page.locator('main')).toBeVisible();
    // The list arrives asynchronously; asserting before it lands would make
    // this test pass for the wrong reason, which is worse than failing.
    await page.waitForLoadState('networkidle');
    await assertNothingSticksOut(page);
  }
});
