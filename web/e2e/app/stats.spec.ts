/**
 * The wind statistics on a station page: made from the archives (stand-ins for the IEM's METAR
 * archive and the DWD's hourly archive) on the first visit, then the windy days per month and
 * the rose of the month picked, as the API has them.
 */
import { expect, test } from './fixtures';

const de = (share: number) => `${Math.round(share * 100)} %`;

test("an airport's statistics: the windy days per month and the month's rose", async ({ page, request }) => {
  await page.goto('/station/lgrp');
  const card = page.getByRole('article', { name: 'Windstatistik' });
  await card.scrollIntoViewIfNeeded();
  // Made on the first visit: ten years of hours, a request per year
  await expect(card.getByText('METAR-Archiv des Iowa Environmental Mesonet')).toBeVisible({ timeout: 45_000 });
  const { stats } = await (await request.get('/api/stations/lgrp/stats')).json();
  expect(stats.months).toHaveLength(12);
  const july = stats.months[6];
  // The stand-in's summer westerly at Rhodes: July windier than January
  expect(july.windyDays / july.days).toBeGreaterThan(stats.months[0].windyDays / stats.months[0].days);
  const bar = card.getByRole('button', { name: `Juli: ${de(july.windyDays / july.days)} windige Tage` });
  await bar.click();
  await expect(bar).toHaveAttribute('aria-pressed', 'true');
  await expect(card.getByRole('heading', { name: 'Juli' })).toBeVisible();
  await expect(card.getByRole('img', { name: /Windrose, Hauptrichtung WSW/ })).toBeVisible();
  // The whole year
  await card.getByRole('button', { name: 'Ganzes Jahr' }).click();
  await expect(card.getByRole('heading', { name: 'Ganzes Jahr' })).toBeVisible();
});

test("a DWD station's statistics from the DWD's archive", async ({ page }) => {
  await page.goto('/station/dwd-04177');
  const card = page.getByRole('article', { name: 'Windstatistik' });
  await card.scrollIntoViewIfNeeded();
  await expect(card.getByText('Klimaarchiv des DWD')).toBeVisible({ timeout: 45_000 });
  await expect(card.getByRole('img', { name: /Windrose, Hauptrichtung SW/ })).toBeVisible();
});
