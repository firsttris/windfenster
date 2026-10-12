/**
 * The app's pages against the real server and the stand-in: what a station page, a DWD station,
 * the search and a point show, with the numbers the API gives for them.
 */
import { expect, test } from './fixtures';

/** The number as the app writes it in German */
const de = (n: number, digits = 0) =>
  n.toLocaleString('de-DE', { maximumFractionDigits: digits, minimumFractionDigits: digits });

test('the home page', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('link', { name: 'Anmelden' })).toBeVisible();
  await expect(page.getByRole('banner').getByRole('button', { name: /Station, Ort oder Koordinaten/ })).toBeVisible();
});

test('a METAR station: measured now, the TAF, the forecast of every model', async ({ page, request }) => {
  await page.goto('/station/edsb');
  await expect(page.getByRole('heading', { level: 1 })).toContainText('Karlsruhe Baden-Baden Airport');
  // Now measured: the newest METAR's wind, as the API has it
  const live = await (await request.get('/api/stations/edsb/live')).json();
  const newest = live.observations.at(-1);
  const now = page.getByRole('article', { name: 'Jetzt gemessen' });
  await expect(now).toContainText(de(newest.wind));
  // The TAF, raw and in words
  const taf = page.getByRole('region', { name: 'Flugwetter-Vorhersage (TAF)' });
  await expect(taf).toContainText('TEMPO');
  // Every model the forecast has
  const station = await (await request.get('/api/stations/edsb')).json();
  const chooser = page.getByRole('region', { name: 'Wettermodell wählen' });
  for (const model of station.forecast.models)
    await expect(chooser.getByRole('button', { name: new RegExp(`^${model.name} `) })).toBeVisible();
  await expect(page.getByRole('table', { name: /Vorhersage pro Tag und Uhrzeit/ })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Messwerte' })).toBeVisible();
});

/** Unix seconds of `hour` o'clock in Berlin tomorrow */
function tomorrowAt(hour: number): number {
  const fmt = new Intl.DateTimeFormat('en-CA', {
    timeZone: 'Europe/Berlin',
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    hourCycle: 'h23',
  });
  const day = (t: number) => fmt.format(new Date(t * 1000)).slice(0, 10);
  const now = Math.floor(Date.now() / 3_600_000) * 3_600;
  const tomorrow = day(now + 86_400);
  for (let t = now; t < now + 3 * 86_400; t += 3_600) {
    const parts = fmt.formatToParts(new Date(t * 1000));
    if (day(t) === tomorrow && Number(parts.find((x) => x.type === 'hour')?.value) === hour) return t;
  }
  throw new Error('no such hour');
}

test("the table shows the chosen model's forecast hour by hour, wind and gusts", async ({ page, request }) => {
  await page.goto('/station/edsb');
  const station = await (await request.get('/api/stations/edsb')).json();
  const d2 = station.forecast.series.find((s: { model: string }) => s.model === 'icon_d2');
  const table = page.getByRole('table', { name: /Vorhersage pro Tag und Uhrzeit/ });
  const row = (name: string) =>
    table
      .getByRole('row')
      .filter({ has: page.getByRole('rowheader', { name }) })
      .getByRole('cell');
  // Tomorrow 07 to 19 o'clock, the second day's 13 columns (ICON-D2, hourly)
  for (const [name, values] of [
    ['Vorhersagekn', d2.wind],
    ['Böen', d2.gust],
  ] as const) {
    // Ten days of columns: wait for all of them, the table fills in as the data comes
    await expect.poll(() => row(name).count()).toBeGreaterThan(40);
    const cells = await row(name).allInnerTexts();
    for (let h = 7; h <= 19; h++) {
      const want = values[(tomorrowAt(h) - d2.start) / d2.step];
      expect(cells[13 + h - 7].trim(), `${name} at ${h}:00 tomorrow`).toBe(String(Math.round(want)));
    }
  }
});

test('a DWD station: ten-minute values', async ({ page, request }) => {
  await page.goto('/station/dwd-04177');
  await expect(page.getByRole('heading', { level: 1 })).toContainText('Rheinstetten');
  const live = await (await request.get('/api/stations/dwd-04177/live')).json();
  await expect(page.getByRole('article', { name: 'Jetzt gemessen' })).toContainText(de(live.observations.at(-1).wind));
});

test('the search finds stations and places, and a place has a forecast', async ({ page }) => {
  await page.goto('/');
  await page
    .getByRole('banner')
    .getByRole('button', { name: /Station, Ort oder Koordinaten/ })
    .click();
  await page.getByPlaceholder('Station, Ort oder Koordinaten …').fill('Rhein');
  await expect(page.getByRole('option', { name: /Rheinstetten/ })).toBeVisible();
  await expect(page.getByRole('option', { name: /Karlsruhe Baden-Baden Airport/ })).toBeVisible();
  await page.getByRole('option', { name: /^Rheinmünster/ }).click();
  await expect(page).toHaveURL(/\/point\?/);
  await expect(page.getByRole('heading', { name: '10-Tage-Vorhersage' })).toBeVisible();
});

test('the sources page, from the footer', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('contentinfo').getByRole('link', { name: 'Datenquellen' }).click();
  await expect(page).toHaveURL(/\/sources$/);
  await expect(page.getByRole('heading', { level: 1, name: 'Datenquellen' })).toBeVisible();
  for (const name of [
    'Open-Meteo',
    'Deutscher Wetterdienst (DWD)',
    'ECMWF Open Data',
    'PEGELONLINE (WSV)',
    'OpenStreetMap',
  ])
    await expect(page.getByRole('main').getByRole('link', { name, exact: true })).toBeVisible();
});
