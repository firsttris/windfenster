/**
 * The wind map's particles on the GPU (`particle-passes.ts` with the shaders' UPDATE_FRAG and
 * DRAW_VERT): where new particles start, how far and which way the wind takes them, that those
 * leaving the wind or the view start anew, and that a particle starting anew draws no stroke.
 * In calm air an earlier hash put new particles on a few points, and their first strokes drew
 * stars there; these tests would have caught it.
 */
import { expect, type Page, test } from '@playwright/test';
import type { Layer, ParticleResult, ParticleRun } from './gpu';
import { hour } from './textures';

// A view over 0–20° E and 45–55° N, 1000 × 800 CSS pixels
const mercX = (lon: number) => (lon + 180) / 360;
const mercY = (lat: number) => {
  const s = Math.sin((lat * Math.PI) / 180);
  return 0.5 - Math.log((1 + s) / (1 - s)) / (4 * Math.PI);
};
const VIEW = { x: mercX(0), y: mercY(55), w: mercX(20) - mercX(0), h: mercY(45) - mercY(55) };
const CSS = { cssWidth: 1000, cssHeight: 800 };
const RES = 128;
const PX_PER_KN = 0.08;

/** One hour of the same wind everywhere over the view and beyond */
function steady(u: number, v: number): Layer {
  const h = hour('ecmwf_ifs025', [-180.5, -90.5, 179.5, 90.5], 1, () => ({ u, v, gust: null }), false);
  return { a: h, b: h, t: 0 };
}

/** Places on a grid inside the view, away from its edges */
function grid(margin = 0.1): number[] {
  const out: number[] = [];
  for (let j = 0; j < RES; j++) {
    for (let i = 0; i < RES; i++) {
      out.push(margin + ((1 - 2 * margin) * (i + 0.5)) / RES, margin + ((1 - 2 * margin) * (j + 0.5)) / RES);
    }
  }
  return out;
}

test.beforeEach(async ({ page }) => {
  await page.goto('/e2e/gpu.html');
  await page.waitForFunction(() => typeof window.particleRun === 'function');
});

function run(
  page: Page,
  layers: { base: Layer | null; mid: Layer | null; detail: Layer | null },
  r: Partial<ParticleRun> & Pick<ParticleRun, 'start' | 'steps'>,
): Promise<ParticleResult> {
  const full: ParticleRun = { res: RES, view: VIEW, ...CSS, ...r };
  return page.evaluate(([l, x]) => window.particleRun(l, x), [layers, full] as const);
}

const calm = { base: steady(0, 0), mid: null, detail: null };
/** A step of the position encoding (two bytes per axis) */
const ONE = 1 / 255 / 255;

/** Pearson's chi-square of places over `n` × `n` equal cells */
function chiSquare(places: number[], n: number): { chi: number; df: number } {
  const counts = new Array(n * n).fill(0);
  for (let i = 0; i < places.length; i += 2) {
    const cx = Math.min(n - 1, Math.floor(places[i] * n));
    const cy = Math.min(n - 1, Math.floor(places[i + 1] * n));
    counts[cy * n + cx] += 1;
  }
  const expected = places.length / 2 / (n * n);
  return { chi: counts.reduce((s, c) => s + (c - expected) ** 2 / expected, 0), df: n * n - 1 };
}

test('new particles spread evenly over the view, also in calm air', async ({ page }) => {
  // Every particle starts anew in each of four frames: 65,536 new places
  const { states } = await run(page, calm, {
    start: grid(),
    steps: [1, 2, 3, 4].map((frame) => ({ pxPerKn: PX_PER_KN, drop: 1, frame })),
  });
  const all = states.flat();
  // Coarse and fine cells, so neither clumps nor points on a few lines (as x and y from one
  // random number made) pass: chi-square within five standard deviations of its mean
  for (const n of [16, 64]) {
    const { chi, df } = chiSquare(all, n);
    expect(chi, `${n} × ${n} cells`).toBeLessThan(df + 5 * Math.sqrt(2 * df));
  }
  // x and y independent of each other
  const xs = all.filter((_, i) => i % 2 === 0);
  const ys = all.filter((_, i) => i % 2 === 1);
  const mean = (a: number[]) => a.reduce((s, x) => s + x, 0) / a.length;
  const [mx, my] = [mean(xs), mean(ys)];
  const cov = mean(xs.map((x, i) => (x - mx) * (ys[i] - my)));
  expect(Math.abs(cov / (1 / 12)), 'correlation of x and y').toBeLessThan(0.02);
  // A new frame, new places: no two frames alike
  const same = states[0].filter((v, i) => Math.abs(v - states[1][i]) < ONE).length;
  expect(same / states[0].length).toBeLessThan(0.01);
});

test('the wind moves each particle its way and speed', async ({ page }) => {
  // 20 kn from the west and 10 kn from the south: east and north, screen y points down
  const start = grid(0.2);
  const { states } = await run(
    page,
    { base: steady(20, 10), mid: null, detail: null },
    {
      start,
      steps: [{ pxPerKn: PX_PER_KN, drop: 0, frame: 1 }],
    },
  );
  const [dx, dy] = [(20 * PX_PER_KN) / CSS.cssWidth, (-10 * PX_PER_KN) / CSS.cssHeight];
  let worst = 0;
  for (let i = 0; i < start.length; i += 2) {
    worst = Math.max(worst, Math.abs(states[0][i] - start[i] - dx), Math.abs(states[0][i + 1] - start[i + 1] - dy));
  }
  // Within the rounding of the positions' encoding
  expect(worst).toBeLessThan(2 * ONE);
});

test('few particles start anew in calm air, more in strong wind', async ({ page }) => {
  const start = grid(0.2);
  const renewed = (r: ParticleResult, dx: number) =>
    r.states[0].filter((v, i) => i % 2 === 0 && Math.abs(v - start[i] - dx) > 4 * ONE).length / (start.length / 2);
  const calmRun = await run(page, calm, { start, steps: [{ pxPerKn: PX_PER_KN, drop: 0.05, frame: 7 }] });
  expect(renewed(calmRun, 0)).toBeGreaterThan(0.04);
  expect(renewed(calmRun, 0)).toBeLessThan(0.06);
  // At 30 kn and more four times as many (UPDATE_FRAG: drop + 3 drop × smoothstep(0, 30, speed))
  const windy = await run(
    page,
    { base: steady(30, 0), mid: null, detail: null },
    {
      start,
      steps: [{ pxPerKn: PX_PER_KN, drop: 0.05, frame: 7 }],
    },
  );
  const share = renewed(windy, (30 * PX_PER_KN) / CSS.cssWidth);
  expect(share).toBeGreaterThan(0.18);
  expect(share).toBeLessThan(0.22);
});

test('particles leaving the wind or the view start anew inside it', async ({ page }) => {
  // ICON-D2 alone over 5–15° E, its domain ending at 12° E; no model around it
  const d2 = hour('icon_d2', [5, 45, 15, 55], 0.05, (lon) => (lon < 12 ? { u: 10, v: 0, gust: null } : null), false);
  const detail = { a: d2, b: d2, t: 0 };
  // Half the particles at 8° E (inside), half at 16° E (no wind there); a column at the
  // view's east edge in a westerly
  const start: number[] = [];
  for (let k = 0; k < RES * RES; k++) start.push(k % 2 ? 0.4 : 0.8, 0.3 + (0.4 * k) / (RES * RES));
  const { states } = await run(
    page,
    { base: null, mid: null, detail },
    {
      start,
      steps: [{ pxPerKn: PX_PER_KN, drop: 0, frame: 3 }],
    },
  );
  const dx = (10 * PX_PER_KN) / CSS.cssWidth;
  let [moved, renewed] = [0, 0];
  for (let k = 0; k < RES * RES; k += 1) {
    const [x, y] = [states[0][k * 2], states[0][k * 2 + 1]];
    if (k % 2) {
      // Inside: moved with the wind
      if (Math.abs(x - start[k * 2] - dx) < 2 * ONE && Math.abs(y - start[k * 2 + 1]) < 2 * ONE) moved += 1;
    } else if (Math.abs(x - 0.8) + Math.abs(y - start[k * 2 + 1]) > 4 * ONE) {
      // Outside: somewhere new
      renewed += 1;
    }
  }
  expect([moved, renewed]).toEqual([(RES * RES) / 2, (RES * RES) / 2]);
  // Leaving the view on the east: anew
  const edge = await run(
    page,
    { base: steady(30, 0), mid: null, detail: null },
    {
      start: Array.from({ length: RES * RES * 2 }, (_, i) => (i % 2 ? 0.5 : 0.9999)),
      steps: [{ pxPerKn: PX_PER_KN, drop: 0, frame: 5 }],
    },
  );
  const xs = edge.states[0].filter((_, i) => i % 2 === 0);
  // All anew, spread over the view: about as many at the east edge as anywhere
  expect(xs.filter((x) => x > 0.99).length / xs.length).toBeLessThan(0.02);
  expect(new Set(xs.map((x) => Math.round(x * 100))).size).toBeGreaterThan(90);
});

/** The drawn particles as strokes: for each lit area its pixels and its longest side */
function strokes(alpha: Uint8Array, width: number, height: number): { pixels: number; extent: number }[] {
  const seen = new Uint8Array(alpha.length);
  const out: { pixels: number; extent: number }[] = [];
  for (let start = 0; start < alpha.length; start++) {
    if (alpha[start] === 0 || seen[start]) continue;
    const stack = [start];
    seen[start] = 1;
    let [x0, x1, y0, y1, pixels] = [width, 0, height, 0, 0];
    while (stack.length) {
      const p = stack.pop() as number;
      const [x, y] = [p % width, Math.floor(p / width)];
      [x0, x1, y0, y1, pixels] = [Math.min(x0, x), Math.max(x1, x), Math.min(y0, y), Math.max(y1, y), pixels + 1];
      for (const [nx, ny] of [
        [x + 1, y],
        [x - 1, y],
        [x, y + 1],
        [x, y - 1],
      ]) {
        const q = ny * width + nx;
        if (nx >= 0 && nx < width && ny >= 0 && ny < height && alpha[q] && !seen[q]) {
          seen[q] = 1;
          stack.push(q);
        }
      }
    }
    out.push({ pixels, extent: Math.max(x1 - x0, y1 - y0) + 1 });
  }
  return out;
}

const DRAW = { width: 1000, height: 800, lineWidth: 1, move: PX_PER_KN };

async function drawn(page: Page, layers: Parameters<typeof run>[1], drop: number) {
  // Few particles, so their strokes rarely touch
  const res = 24;
  const start = Array.from({ length: res * res * 2 }, (_, i) => 0.05 + 0.9 * ((i * 0.618034) % 1));
  const r = await run(page, layers, { res, start, steps: [{ pxPerKn: PX_PER_KN, drop, frame: 9 }], draw: DRAW });
  const alpha = Uint8Array.from(Buffer.from(r.alpha ?? '', 'base64'));
  return strokes(alpha, DRAW.width, DRAW.height);
}

test('a particle starting anew draws no stroke to its new place', async ({ page }) => {
  // Every particle jumps somewhere new, in calm air and in a gale: dots only
  for (const layers of [calm, { base: steady(30, 0), mid: null, detail: null }]) {
    const all = await drawn(page, layers, 1);
    expect(all.length).toBeGreaterThan(400);
    expect(Math.max(...all.map((s) => s.extent)), 'longest stroke').toBeLessThanOrEqual(4);
  }
});

test('moving particles draw a short stroke along the wind', async ({ page }) => {
  // 30 kn east: 2.4 px a frame
  const all = await drawn(page, { base: steady(30, 0), mid: null, detail: null }, 0);
  const lengths = all.map((s) => s.extent).sort((a, b) => a - b);
  const median = lengths[Math.floor(lengths.length / 2)];
  expect(median).toBeGreaterThanOrEqual(3);
  expect(median).toBeLessThanOrEqual(5);
  // In calm air: dots
  const still = await drawn(page, calm, 0);
  expect(Math.max(...still.map((s) => s.extent))).toBeLessThanOrEqual(2);
});
