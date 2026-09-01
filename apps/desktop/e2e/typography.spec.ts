import { expect, test } from '@playwright/test';

/**
 * The bundled typeface actually arrives.
 *
 * A font that fails to load does not throw and does not look broken — it looks
 * like a slightly different application, and the fallback stack is good enough
 * that nobody files a bug. That makes it precisely the kind of regression a
 * path change or a bundler upgrade introduces silently, so the four weights are
 * asserted rather than assumed.
 */
test('all four Google Sans weights load from the bundle', async ({ page }) => {
  await page.goto('/#/');

  const loaded = await page.evaluate(async () => {
    await document.fonts.ready;
    return [...document.fonts]
      .filter((face) => face.family === 'Google Sans' && face.status === 'loaded')
      .map((face) => face.weight)
      .sort();
  });

  // Regular, Medium, SemiBold, Bold — the weight contrast the whole layout
  // leans on. Losing SemiBold silently collapses headings into body text.
  expect(loaded).toEqual(['400', '500', '600', '700']);
});

test('the heading is set in the bundled face, not the fallback', async ({ page }) => {
  await page.goto('/#/');

  const heading = page.getByRole('heading', { name: 'את מי מחפשים?' });
  await expect(heading).toBeVisible();

  const rendered = await heading.evaluate(async (element) => {
    await document.fonts.ready;
    const style = getComputedStyle(element);
    return {
      family: style.fontFamily,
      weight: style.fontWeight,
      // `document.fonts.check` answers whether the text would actually be
      // painted with the face, which is the part a font-family string cannot
      // tell you on its own.
      usesGoogleSans: document.fonts.check(
        `${style.fontWeight} 1rem "Google Sans"`,
        element.textContent ?? '',
      ),
    };
  });

  expect(rendered.family).toContain('Google Sans');
  expect(rendered.weight).toBe('700');
  expect(rendered.usesGoogleSans).toBe(true);
});

test('the home screen offers example searches and running one finds people', async ({ page }) => {
  // The point of the chips: someone who has forgotten the name should not have
  // to guess what the box accepts.
  await page.goto('/#/');
  await expect(page.getByText('לא זוכרים את השם?')).toBeVisible();

  await page.getByRole('button', { name: /בורו פארק/ }).click();

  await expect(page.getByRole('textbox', { name: 'חיפוש אנשי קשר' })).toHaveValue('בורו פארק');
  await expect(page.getByRole('link', { name: /רוזנברג/ }).first()).toBeVisible();

  // And the chips step aside once there is a query — they are a starting
  // point, not permanent furniture on the results screen.
  await expect(page.getByText('לא זוכרים את השם?')).toBeHidden();
});
