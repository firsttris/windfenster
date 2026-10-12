/**
 * The wind map against the real server: the models it offers and draws, the textures it loads
 * (ICON global in pieces around the view), a spot's hours as the API has them, picking a place
 * and an hour, the model and the gust layer.
 */
import type { Page } from '@playwright/test';
import { expect, test } from './fixtures';

const EDSB = '/map?spot=48.7794%2C8.0805&station=edsb#8/48.7794/8.0805';

/** Waits for a texture of `model` (a piece for ICON global) and gives its path. The map loads
 * in 5 to 10 seconds on a software GPU; ICON global's piece follows once it knows its view. */
async function texture(page: Page, model: string): Promise<string> {
  const r = await page.waitForResponse(
    (r) => r.ok() && new RegExp(`^/api/map/${model}/\\d{10}/\\d{3}(/[-\\d_]+)?\\.png$`).test(new URL(r.url()).pathname),
    { timeout: 45_000 },
  );
  return new URL(r.url()).pathname;
}

test('the map offers and draws all three models', async ({ page }) => {
  const loading = Promise.all(['icon_d2', 'ecmwf_ifs025', 'icon_global'].map((m) => texture(page, m)));
  await page.goto('/map#7/48.5/9');
  await expect(page.getByRole('heading', { name: 'Wind weltweit' })).toBeVisible();
  const models = page.getByRole('group', { name: 'Modell' });
  for (const chip of ['ICON-D2 2,2 km', 'ICON global 13 km', 'ECMWF IFS 25 km'])
    await expect(models.getByRole('button', { name: chip })).toBeVisible();
  await expect(page.getByText('ICON-D2 › ICON global › ECMWF IFS')).toBeVisible();
  await expect(page.getByText(/ICON-D2 · Lauf \d\d UTC · DWD/)).toBeVisible();
  const [, , piece] = await loading;
  // ICON global in a piece around the view (about 7° × 4° at zoom 7, a quarter more each side,
  // edges on whole 10°): 0–20° E, 40–60° N
  expect(piece).toMatch(/\/0_40_20_60\.png$/);
});

test("a spot's hours are the ones the API has for it", async ({ page, request }) => {
  await page.goto(EDSB);
  const card = page.getByRole('region', { name: 'Karlsruhe Baden-Baden Airport' });
  await expect(card).toBeVisible();
  // As the app asks: on the 0.01° grid places are stored on (lib/api.ts, mapSeriesQuery). The exact
  // place gives slightly different values, which round apart near a half knot.
  const series = await (await request.get('/api/map/series?lat=48.78&lon=8.08')).json();
  const d2 = new Map<number, { wind: number; gust: number | null }>(
    series.layers
      .find((l: { model: string }) => l.model === 'icon_d2')
      .points.map((p: { validAt: number; wind: number; gust: number | null }) => [p.validAt, p]),
  );
  const hours = card.getByRole('group', { name: 'Die nächsten Stunden an diesem Ort' }).getByRole('button');
  await expect(hours.first()).toBeVisible();
  const labels = await hours.evaluateAll((els) => els.map((e) => e.getAttribute('aria-label') ?? ''));
  expect(labels.length).toBeGreaterThan(40);
  // The first 24 hours, ICON-D2 at EDSB, labelled like "Mo., 12., 11:00, 16 kn, Böen 26 kn, SW"
  // Matched by day of the month and hour in Berlin: numbers, which every locale writes alike
  const parts = new Intl.DateTimeFormat('en-CA', {
    timeZone: 'Europe/Berlin',
    day: 'numeric',
    hour: '2-digit',
    hourCycle: 'h23',
  });
  const key = (t: number) => {
    const p = parts.formatToParts(new Date(t * 1000));
    return `${Number(p.find((x) => x.type === 'day')?.value)} ${p.find((x) => x.type === 'hour')?.value}`;
  };
  const byName = new Map([...d2.entries()].map(([t, p]) => [key(t), p]));
  let compared = 0;
  for (const label of labels.slice(0, 24)) {
    const [, day, hour, kn, gust] = label.match(/^[^,]+, (\d+)\., (\d\d):00, (\d+) kn, Böen (\d+) kn/) ?? [];
    const when = `${Number(day)} ${hour}`;
    const p = byName.get(when);
    if (!p) continue;
    expect(Number(kn), label).toBe(Math.round(p.wind));
    expect(Number(gust), label).toBe(Math.round(Math.max(p.gust ?? 0, p.wind)));
    compared += 1;
  }
  expect(compared).toBeGreaterThanOrEqual(20);
});

test('picking an hour moves the map to it; closing the spot', async ({ page }) => {
  await page.goto(EDSB);
  const card = page.getByRole('region', { name: 'Karlsruhe Baden-Baden Airport' });
  const shown = page
    .getByRole('complementary')
    .getByText(/^\w\w\., \d+\. \w+\.?, \d\d:\d\d$/)
    .first();
  await expect(shown).toBeVisible();
  // The next 15:00 in the strip, then the map's time is 15:00
  const hour = card.getByRole('button', { name: /, 15:00, / }).first();
  await hour.click();
  await expect(shown).toHaveText(/, 15:00$/);
  await card.getByRole('button', { name: 'Schließen' }).click();
  await expect(card).toBeHidden();
});

test('a place picked on the map, the model and the gust layer', async ({ page }) => {
  await page.goto('/map#7/48.5/9');
  const map = page.locator('.maplibregl-canvas');
  await expect(map).toBeVisible();
  // Clicks count once the map has loaded: its first texture of ICON global follows that
  await texture(page, 'icon_global');
  const box = await map.boundingBox();
  if (!box) throw new Error('no map');
  await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
  // A spot card for the place, with the wind there
  const card = page
    .getByRole('region')
    .filter({ has: page.getByRole('heading', { level: 2 }) })
    .filter({ hasText: '° N' });
  await expect(card.first()).toBeVisible();
  await expect(card.first()).toContainText(/\d+ kn|Böen \d+ kn/);
  // ECMWF alone: chosen, in the address, its run named
  await page.getByRole('group', { name: 'Modell' }).getByRole('button', { name: 'ECMWF IFS 25 km' }).click();
  await expect(page).toHaveURL(/model=ecmwf_ifs025/);
  await expect(page.getByText(/ECMWF IFS · Lauf \d\d UTC · ECMWF/)).toBeVisible();
  await expect(page.getByText('ICON-D2 › ICON global › ECMWF IFS')).toBeHidden();
  // The gusts
  const layer = page.getByRole('group', { name: 'Ebene' });
  await layer.getByRole('button', { name: 'Böen' }).click();
  await expect(layer.getByRole('button', { name: 'Böen' })).toHaveAttribute('aria-pressed', 'true');
});

test('the water gauges with their levels, from town zoom on', async ({ page }) => {
  const asked = page.waitForResponse((r) => r.ok() && new URL(r.url()).pathname === '/api/map/gauges');
  await page.goto('/map#10/48.9/8.15');
  const gauges = await (await asked).json();
  // Imported with the stations; the one abroad without coordinates left out
  expect(gauges.map((g: { name: string; levelCm: number; state: string }) => [g.name, g.levelCm, g.state])).toEqual([
    ['Maxau', 498, 'normal'],
    ['Iffezheim', 612, 'high'],
  ]);
  await expect(page.getByText(/blau normal, gelb niedrig, rot hoch/)).toBeVisible();
  await expect(page.getByRole('link', { name: 'PEGELONLINE (WSV)' })).toBeVisible();
});

test('the map fills the window under a floating header; the panel folds away, a spot opens it', async ({ page }) => {
  await page.goto('/map#7/48.5/9');
  const map = page.locator('.maplibregl-canvas');
  await expect(map).toBeVisible();
  // From the window's top edge to its bottom, under the header
  const box = await map.boundingBox();
  const size = page.viewportSize();
  expect(box?.y).toBe(0);
  expect(box?.height).toBe(size?.height);
  await expect(page.getByRole('banner')).toBeVisible();
  // Folded away, and so after a reload
  const title = page.getByRole('heading', { name: 'Wind weltweit' });
  await expect(title).toBeVisible();
  await page.getByRole('button', { name: 'Seitenleiste einklappen' }).click();
  await expect(title).toBeHidden();
  await page.reload();
  const open = page.getByRole('button', { name: 'Seitenleiste zeigen' });
  await expect(open).toBeVisible();
  await expect(title).toBeHidden();
  // A place picked on the map opens it with the spot
  await texture(page, 'icon_global');
  const now = await map.boundingBox();
  if (!now) throw new Error('no map');
  await page.mouse.click(now.x + now.width / 2, now.y + now.height / 2);
  await expect(title).toBeVisible();
  await expect(page.getByRole('region').filter({ hasText: '° N' }).first()).toBeVisible();
  await expect(open).toBeHidden();
});
