/**
 * The ranking on the model chips: place and error inside every chip at every width and unit, and
 * the legend that says what they are and leads to the ranking card. The stand-in verifies
 * nothing, so the station's answer gets a ranking here.
 */
import type { Page } from '@playwright/test';
import { expect, test } from './fixtures';

/** A ranking over the station's models and the TAF, errors from 1.8 kn up; `days` for all */
async function withRanking(page: Page, days: number) {
  await page.route('**/api/stations/edsb', async (route) => {
    // Node's fetch: the request goes to the server as it is, only the answer changes
    const body = await (await fetch(route.request().url())).json();
    const ids: string[] = [...body.forecast.models.map((m: { id: string }) => m.id), 'taf'];
    const scores = ids.map((model, i) => ({
      model,
      n: 300,
      days,
      mae: 1.8 + i * 0.4,
      bias: 0,
      dirError: null,
      hits: 0,
      misses: 0,
      falseAlarms: 0,
    }));
    body.ranking = { days, scores, bestSuper: null, bestGlobal: null, mixFirst: false };
    await route.fulfill({ json: body });
  });
}

for (const unit of ['kn', 'ms', 'kmh']) {
  test(`the places stay inside their chips from lg on, in ${unit}`, async ({ page }) => {
    await page.addInitScript((u) => localStorage.setItem('wf.unit', u), unit);
    await withRanking(page, 20);
    for (const width of [1024, 1100, 1280, 1440]) {
      await page.setViewportSize({ width, height: 900 });
      await page.goto('/station/edsb');
      const chips = page.getByRole('region', { name: 'Wettermodell wählen' }).getByRole('button');
      await expect(chips.first()).toContainText('#');
      const over = await chips.evaluateAll((buttons) =>
        buttons.filter((b) => b.scrollWidth > b.clientWidth).map((b) => b.textContent),
      );
      expect(over, `chips too narrow at ${width} px`).toEqual([]);
    }
  });
}

test('the legend says what the places are and leads to the ranking card', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await withRanking(page, 20);
  await page.goto('/station/edsb');
  const region = page.getByRole('region', { name: 'Wettermodell wählen' });
  await expect(region).toContainText('# Platz · Ø Abweichung');
  await region.getByRole('link', { name: 'Wer trifft an EDSB? ↓' }).click();
  await expect(page).toHaveURL(/#ranking$/);
  await expect(page.getByRole('article', { name: 'Wer trifft an EDSB?' })).toBeInViewport();
});

test('under 14 days the legend says so', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await withRanking(page, 6);
  await page.goto('/station/edsb');
  await expect(page.getByRole('region', { name: 'Wettermodell wählen' })).toContainText('# Platz, erst 6 von 14 Tagen');
});
