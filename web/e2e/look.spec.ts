/**
 * The wind map's look, compared with approved pictures: the colour field and the particles with
 * their trails, painted by the map's own code from fixed wind and fixed random numbers. Catches
 * what nobody wrote a test for: stars, seams between the models, stripes, a wrong colour scale.
 *
 * A change that is meant to alter the look updates the pictures: `pnpm e2e --update-snapshots`,
 * then look at them before committing. They are made in the Playwright image CI runs in; another
 * Chromium may paint a few pixels differently.
 */
import { expect, type Page, test } from '@playwright/test';
import type { Layer, Scene } from './gpu';
import { hour, type Value } from './textures';

const cos = (deg: number) => Math.cos((deg * Math.PI) / 180);
const sin = (deg: number) => Math.sin((deg * Math.PI) / 180);

/** A low over 10° E 50° N: 25 kn at its strongest, 5° from a calm eye, turning anticlockwise,
 * over a light westerly */
function storm(lon: number, lat: number): { u: number; v: number } {
  const [dx, dy] = [(lon - 10) * cos(50), lat - 50];
  const r = Math.hypot(dx, dy) || 1e-6;
  const speed = 25 * (r / 5) * Math.exp(1 - r / 5);
  return { u: 4 - (speed * dy) / r, v: (speed * dx) / r };
}

/** Eddies without sources or sinks (from a stream function), `a` kn at most, `k` degrees of
 * phase per degree: where air neither gathers nor spreads, particles must not either. */
function eddies(lon: number, lat: number, a: number, k: number): { u: number; v: number } {
  return { u: -a * sin(lon * k) * cos(lat * k), v: a * cos(lon * k) * sin(lat * k) };
}

const withGust = (w: { u: number; v: number }): Value => ({ ...w, gust: 1.4 * Math.hypot(w.u, w.v) + 3 });

/** The three models as the map lays them: ECMWF around the world, ICON global in a piece, and
 * ICON-D2 with an uneven domain edge; the finer ones add eddies of their own, `detail` kn. */
function models(field: (lon: number, lat: number) => { u: number; v: number }, detail: number) {
  const plus = (lon: number, lat: number, a: number, k: number) => {
    const [w, e] = [field(lon, lat), eddies(lon, lat, a, k)];
    return withGust({ u: w.u + e.u, v: w.v + e.v });
  };
  const ecmwf = hour('ecmwf_ifs025', [-180.5, -90.5, 179.5, 90.5], 1, (lon, lat) => withGust(field(lon, lat)), true);
  const global = hour('icon_global', [-10, 30, 40, 70], 0.125, (lon, lat) => plus(lon, lat, detail / 2, 45), true);
  const d2 = hour(
    'icon_d2',
    [2, 43, 20, 56],
    0.05,
    (lon, lat) => (lon < 3 || lon > 19 || lat < 44 || lat > 55 - 0.25 * (lon - 3) ? null : plus(lon, lat, detail, 160)),
    true,
  );
  const one = (h: typeof ecmwf): Layer => ({ a: h, b: h, t: 0 });
  return { base: one(ecmwf), mid: one(global), detail: one(d2) };
}

const SCENE: Omit<Scene, 'gusts'> = { width: 800, height: 500, bounds: [-6, 38, 26, 60], frames: 90 };

test.beforeEach(async ({ page }) => {
  await page.goto('/e2e/gpu.html');
  await page.waitForFunction(() => typeof window.scene === 'function');
});

/** The scene as a PNG, read from the canvas itself (a screenshot waits for the page to hold
 * still, which takes long on a software GPU) */
async function paint(page: Page, layers: ReturnType<typeof models>, gusts: boolean): Promise<Buffer> {
  await page.evaluate(([l, s]) => window.scene(l, s), [layers, { ...SCENE, gusts }] as const);
  const url = await page.evaluate(() => (document.getElementById('scene') as HTMLCanvasElement).toDataURL('image/png'));
  return Buffer.from(url.split(',')[1], 'base64');
}

// A seam across the view is some 1,500 pixels: fewer than that may differ.
const SAME = { maxDiffPixels: 600, threshold: 0.15 };

test('a low over all three models, wind', async ({ page }) => {
  expect(await paint(page, models(storm, 3), false)).toMatchSnapshot('storm-wind.png', SAME);
});

test('a low over all three models, gusts', async ({ page }) => {
  expect(await paint(page, models(storm, 3), true)).toMatchSnapshot('storm-gusts.png', SAME);
});

test('calm air', async ({ page }) => {
  // Up to 2 kn, the finer models a little different: particles stay spread out, no stars
  const calm = (lon: number, lat: number) => eddies(lon, lat, 2, 50);
  expect(await paint(page, models(calm, 0.4), false)).toMatchSnapshot('calm.png', SAME);
});
