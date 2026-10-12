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
  // Ten days, each a table of its own
  await expect(page.getByRole('table', { name: /^Vorhersage für / })).toHaveCount(10);
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

test("tomorrow's table shows the chosen model hour by hour: wind, the gust under it", async ({ page, request }) => {
  await page.goto('/station/edsb');
  const station = await (await request.get('/api/stations/edsb')).json();
  const d2 = station.forecast.series.find((s: { model: string }) => s.model === 'icon_d2');
  // Tomorrow, the second day
  const table = page.getByRole('table', { name: /^Vorhersage für / }).nth(1);
  const row = (name: RegExp) =>
    table
      .getByRole('row')
      .filter({ has: page.getByRole('rowheader', { name }) })
      .getByRole('cell');
  // 07 to 19 o'clock, ICON-D2 hourly; the gust shown is never below the wind
  await expect.poll(() => row(/^Wind\s*kn$/).count()).toBe(13);
  const winds = await row(/^Wind\s*kn$/).allInnerTexts();
  const gusts = await row(/^Böen$/).allInnerTexts();
  for (let h = 7; h <= 19; h++) {
    const i = (tomorrowAt(h) - d2.start) / d2.step;
    expect(winds[h - 7].trim(), `wind at ${h}:00 tomorrow`).toBe(String(Math.round(d2.wind[i])));
    expect(gusts[h - 7].trim(), `gust at ${h}:00 tomorrow`).toBe(String(Math.round(Math.max(d2.gust[i], d2.wind[i]))));
  }
  // Every 3 hours instead: 8 columns, kept after a reload
  await page.getByRole('group', { name: 'Heute und morgen' }).getByRole('button', { name: '3 h' }).click();
  await expect.poll(() => row(/^Wind\s*kn$/).count()).toBe(8);
  await page.reload();
  await expect.poll(() => row(/^Wind\s*kn$/).count()).toBe(8);
});

test('measured values and the TAF stand under the forecast, to compare', async ({ page, request }) => {
  await page.goto('/station/edsb');
  // Every 3 hours around the clock: the stand-in's TEMPO (the three hours around now) always meets
  // a column, at any time of day; hour by hour today shows only 7 to 19
  await page.getByRole('group', { name: 'Heute und morgen' }).getByRole('button', { name: '3 h' }).click();
  const today = page.getByRole('table', { name: /^Vorhersage für / }).first();
  await expect(today.getByText('Zum Vergleich')).toBeVisible();
  const row = (name: RegExp) =>
    today
      .getByRole('row')
      .filter({ has: page.getByRole('rowheader', { name }) })
      .getByRole('cell');
  // Measured: the hours that are over show values the station reported
  const live = await (await request.get('/api/stations/edsb/live')).json();
  const reported = new Set(live.observations.map((o: { wind: number }) => String(Math.round(o.wind))));
  const measured = (await row(/Gemessen/).allInnerTexts()).map((t) => t.split('/')[0].trim()).filter(Boolean);
  expect(measured.filter((t) => /^\d+$/.test(t)).length).toBeGreaterThan(0);
  for (const t of measured.filter((t) => /^\d+$/.test(t))) expect(reported, `measured ${t}`).toContain(t);
  // TAF: the prevailing 12 kn, the span TEMPO allows up to 18 kn
  const taf = (await row(/^TAF/).allInnerTexts()).map((t) => t.trim());
  expect(taf).toContain('12–18');
  // and what the numbers do not show, in words: the TEMPO's showers
  const notes = page.locator('section[id^="day-"]').getByText(/^TAF · zeitw\./);
  await expect(notes.first()).toBeVisible();
  const card = page
    .locator('section[id^="day-"]')
    .filter({ has: page.getByText(/^TAF · zeitw\./) })
    .first();
  await expect(card).toContainText(/Schauer/);
  // The "?" says what a row is when tapped, for touch screens without hover
  await card.getByRole('button', { name: /^TAF:/ }).click();
  await expect(card.getByRole('note')).toContainText('Flugwetter-Vorhersage');
  await card.getByRole('button', { name: /^TAF:/ }).click();
  await expect(card.getByRole('note')).toHaveCount(0);
});

test('the quick view shows all ten days and leads to each one', async ({ page }) => {
  await page.goto('/station/edsb');
  const glance = page.getByRole('region', { name: 'Auf einen Blick' });
  const days = glance.getByRole('listitem');
  await expect(days).toHaveCount(10);
  // Each day its strongest wind; without a saved spot no hours are marked
  await expect(days.first()).toContainText(/max \d+ kn/);
  await expect(glance).toContainText('Strich = Böe');
  // How far the models agree, in a word
  await expect(days.first()).toContainText(/Hoch|Mittel|Gering/);
  // A day leads to its card
  const sixth = days.nth(5).getByRole('button');
  const title = (await sixth.getAttribute('title'))?.replace(/^Zu (\S+), .* springen$/, '$1') ?? '';
  await sixth.click();
  await expect(page.getByRole('heading', { level: 3, name: title, exact: true })).toBeInViewport();
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
