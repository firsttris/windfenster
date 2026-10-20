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
  // The TAF, folded; opened, raw and in words
  const taf = page.getByRole('region', { name: 'Flugwetter-Vorhersage (TAF)' });
  await expect(taf).not.toContainText('TEMPO');
  await taf.getByRole('button', { name: /Flugwetter-Vorhersage/ }).click();
  await expect(taf).toContainText('TEMPO');
  // Every model the forecast has
  const station = await (await request.get('/api/stations/edsb')).json();
  const chooser = page.getByRole('region', { name: 'Wettermodell wählen' });
  for (const model of station.forecast.models)
    await expect(chooser.getByRole('button', { name: new RegExp(`^${model.name} `) })).toBeVisible();
  // On a desktop they wrap inside the box instead of running out of it
  for (const width of [1024, 1280, 1440]) {
    await page.setViewportSize({ width, height: 900 });
    const right = await chooser.evaluate((el) => el.getBoundingClientRect().right);
    for (const button of await chooser.getByRole('button').all()) {
      const end = await button.evaluate((el) => el.getBoundingClientRect().right);
      expect(end, `${await button.textContent()} at ${width}px`).toBeLessThanOrEqual(right);
    }
  }
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
  // A global model instead: every 3 hours, 8 columns
  await page
    .getByRole('region', { name: 'Wettermodell wählen' })
    .getByRole('button', { name: /^ECMWF IFS/ })
    .click();
  await expect.poll(() => row(/^Wind\s*kn$/).count()).toBe(8);
});

test('the TAF stands under the forecast in sections, what was measured is not in the table', async ({ page }) => {
  await page.goto('/station/edsb');
  // A global model, every 3 hours around the clock: the stand-in's TEMPO (five hours around now)
  // always meets an hour still to come of today or tomorrow; hour by hour today shows only 7 to 19
  await page
    .getByRole('region', { name: 'Wettermodell wählen' })
    .getByRole('button', { name: /^ECMWF IFS/ })
    .click();
  // The measurements are further down, not in the days
  await expect(page.getByRole('rowheader', { name: /Gemessen/ })).toHaveCount(0);
  // TAF: a value once per section, the span TEMPO allows among them
  const taf = (
    await page
      .getByRole('row')
      .filter({ has: page.getByRole('rowheader', { name: /^TAF/ }) })
      .getByRole('cell')
      .allInnerTexts()
  ).map((t) => t.replace(/\s+/g, ' ').trim());
  expect(taf.some((t) => t.startsWith('12–18'))).toBe(true);
  // a section spans its hours: fewer cells than hours in the row
  expect(taf.length).toBeLessThan(8 * 2);
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

test('on a phone each day lists its hours under one another, the strip jumps', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto('/station/edsb');
  // Nothing wider than the phone
  await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth)).toBe(390);
  // The ten days as a strip; the tables are for wider screens
  const strip = page.getByRole('navigation', { name: 'Die zehn Tage' });
  await expect(strip.getByRole('button')).toHaveCount(10);
  await expect(page.getByRole('table', { name: /^Vorhersage für / }).first()).toBeHidden();
  // Tomorrow, the second day: every hour a row with its wind, gust and direction
  const tomorrow = page.locator('section[id^="day-"]').nth(1);
  await expect(tomorrow.getByText(/^Böen \d+$/).first()).toBeVisible();
  // Today's hours that are over stay, shaded, nothing folds them away
  const today = page.locator('section[id^="day-"]').first();
  await expect(today.getByRole('button', { name: /vergangene Stunden/ })).toHaveCount(0);
  // A day in the strip leads to its card
  await strip.getByRole('button').nth(4).click();
  await expect(page.locator('section[id^="day-"]').nth(4)).toBeInViewport();
});

test('the models explained: from the chosen model to its card, and from an area to its models', async ({ page }) => {
  await page.goto('/station/edsb');
  const chosen = page.getByRole('region', { name: 'Modell wählen' }).getByRole('button', { pressed: true });
  const name = (await chosen.locator('span.font-semibold').first().textContent()) ?? '';
  await page.getByRole('link', { name: 'Modelle erklärt' }).first().click();
  await expect(page).toHaveURL(/\/models#/);
  await expect(page.getByRole('heading', { level: 1, name: 'Die Wettermodelle' })).toBeVisible();
  // The chosen model's card is the one in view
  await expect(page.getByRole('article', { name, exact: true })).toBeInViewport();
  // Every model of the server has its card, the fine ones first
  const models = await (await page.request.get('/api/models')).json();
  await expect(page.getByRole('heading', { level: 3 }).filter({ hasNotText: /Automatisch|Mix/ })).toHaveCount(
    models.length,
  );
  // An area leads to its models
  const alps = page.getByRole('row', { name: /Seen in den Alpen/ });
  await alps.getByRole('link', { name: 'ICON-D2' }).click();
  await expect(page).toHaveURL(/\/models#icon_d2$/);
  await expect(page.getByRole('article', { name: 'ICON-D2', exact: true })).toBeInViewport();
});
