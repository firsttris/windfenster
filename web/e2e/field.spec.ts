/**
 * The wind map's colour field on the GPU (`wind-field.ts` with FIELD_FRAG): the colour of the
 * scale for the wind or the gust, the gust never below the wind and the wind where a model has
 * no gusts, nothing where no model has wind, the field's opacity, and its fading out at town zoom.
 */
import { expect, type Page, test } from '@playwright/test';
import { RAMP_MAX_KN, rampPixels } from '../src/lib/wind-map';
import type { Layer } from './gpu';
import { GUST_MAX, hour, RANGE, type Value } from './textures';

const SIZE = { width: 200, height: 100 };
const BOUNDS: [number, number, number, number] = [0, 40, 20, 50];
const MIDDLE: [number, number] = [100, 50];

/** The same wind everywhere; `gusts: false` for a model hour without them */
function steady(w: Value, gusts = true): Layer {
  const h = hour('ecmwf_ifs025', [-180.5, -90.5, 179.5, 90.5], 1, () => w, gusts);
  return { a: h, b: h, t: 0 };
}

/** The scale's colour for `kn`, as the GPU reads the 256 × 1 ramp texture (linear, edges clamped) */
function rampColour(kn: number): number[] {
  const ramp = rampPixels();
  const f = Math.min(1, Math.max(0, kn / RAMP_MAX_KN)) * 256 - 0.5;
  const [i0, i1] = [Math.max(0, Math.floor(f)), Math.min(255, Math.floor(f) + 1)];
  const t = Math.min(1, Math.max(0, f - Math.floor(f)));
  return [0, 1, 2].map((c) => ramp[i0 * 4 + c] * (1 - t) + ramp[i1 * 4 + c] * t);
}

test.beforeEach(async ({ page }) => {
  await page.goto('/e2e/gpu.html');
  await page.waitForFunction(() => typeof window.scene === 'function');
});

async function middle(
  page: Page,
  layers: { base: Layer | null; mid: Layer | null; detail: Layer | null },
  o: { gusts?: boolean; zoom?: number; opacity?: number } = {},
): Promise<number[]> {
  const sc = { ...SIZE, bounds: BOUNDS, frames: 0, clear: true, gusts: false, opacity: 1, ...o };
  await page.evaluate(([l, s]) => window.scene(l, s), [layers, sc] as const);
  const [px] = await page.evaluate((p) => window.pixelsAt(p), [MIDDLE]);
  return px;
}

/** An opaque pixel of the scale's colour for `kn`, within the rounding of 8-bit textures */
function expectColour(px: number[], kn: number, alpha = 255) {
  const want = rampColour(kn).map((c) => (c * alpha) / 255);
  expect(Math.abs(px[3] - alpha), `alpha ${px[3]}`).toBeLessThanOrEqual(3);
  for (const c of [0, 1, 2]) expect(Math.abs(px[c] - want[c]), `${px} for ${kn} kn`).toBeLessThanOrEqual(4);
}

const only = (base: Layer) => ({ base, mid: null, detail: null });

/** A value as the texture holds it: 255 steps between `min` and `max` */
const stored = (x: number, min: number, max: number) =>
  min + (Math.round(((x - min) / (max - min)) * 255) / 255) * (max - min);
/** The speed the texture holds for (u, v) */
const held = (u: number, v: number) => Math.hypot(stored(u, RANGE.uMin, RANGE.uMax), stored(v, RANGE.vMin, RANGE.vMax));

test('the wind in the colour of the scale', async ({ page }) => {
  for (const kn of [0, 5, 12, 22, 35]) {
    // From the south-west: the speed counts, not the direction
    const w = kn / Math.SQRT2;
    expectColour(await middle(page, only(steady({ u: w, v: w, gust: kn + 6 }))), held(w, w));
  }
  // Beyond the scale: its top
  expectColour(await middle(page, only(steady({ u: 55, v: 0, gust: 58 }))), RAMP_MAX_KN);
});

test('the gusts in the colour of the scale', async ({ page }) => {
  expectColour(await middle(page, only(steady({ u: 12, v: 0, gust: 25 })), { gusts: true }), stored(25, 0, GUST_MAX));
  // A gust below the wind shows the wind
  expectColour(await middle(page, only(steady({ u: 12, v: 0, gust: 8 })), { gusts: true }), held(12, 0));
  // A model hour without gusts shows the wind
  expectColour(await middle(page, only(steady({ u: 12, v: 0, gust: null }, false)), { gusts: true }), held(12, 0));
});

test('nothing where no model has wind', async ({ page }) => {
  // ICON-D2 alone, its domain far from the middle
  const d2 = hour('icon_d2', [0, 40, 20, 50], 0.05, (lon) => (lon < 2 ? { u: 10, v: 0, gust: 12 } : null), true);
  const px = await middle(page, { base: null, mid: null, detail: { a: d2, b: d2, t: 0 } });
  expect(px[3]).toBe(0);
});

test('the field fades out from town zoom on', async ({ page }) => {
  const layers = only(steady({ u: 12, v: 0, gust: 18 }));
  // The map's opacity
  expectColour(await middle(page, layers, { opacity: 0.9 }), held(12, 0), 230);
  // Halfway between zoom 9.5 and 11.5: half
  expectColour(await middle(page, layers, { zoom: 10.5 }), held(12, 0), 128);
  // Town zoom: the map alone
  expect((await middle(page, layers, { zoom: 12 }))[3]).toBe(0);
});
