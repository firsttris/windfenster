/**
 * The wind map's layering on the GPU: the shaders' `wind()` and the CPU's `windAt()` (the readout
 * under the pointer, the spot card, the arrows) at places inside, across and outside each model,
 * for wind and gusts, against values known from the textures. Runs in Chromium with SwiftShader;
 * in CI in the official Playwright image.
 */
import { expect, test } from '@playwright/test';
import type { Both, Layer } from './gpu';
import { hour } from './textures';

const cos = (deg: number) => Math.cos((deg * Math.PI) / 180);

// ECMWF around the world, 1°: the wind varies smoothly with the longitude, across 180° too. Its
// first hour has no gusts.
const ecmwf = (lon: number) => ({ u: 10 + 5 * cos(lon), v: -5, gust: 20 });
const base: Layer = {
  a: hour('ecmwf_ifs025', [-180.5, -90.5, 179.5, 90.5], 1, (lon) => ecmwf(lon), false),
  b: hour('ecmwf_ifs025', [-180.5, -90.5, 179.5, 90.5], 1, (lon) => ecmwf(lon), true),
  t: 0.5,
};
// A piece of ICON global, 0–40° E and 30–60° N at 0.125°; its second hour has no gusts.
const piece: [number, number, number, number] = [0, 30, 40, 60];
const mid: Layer = {
  a: hour('icon_global', piece, 0.125, () => ({ u: 20, v: 5, gust: 30 }), true),
  b: hour('icon_global', piece, 0.125, () => ({ u: 20, v: 5, gust: 30 }), false),
  t: 0.5,
};
// ICON-D2 at 0.05°, its domain ending at 12° E inside its texture
const d2 = hour('icon_d2', [5, 45, 15, 55], 0.05, (lon) => (lon < 12 ? { u: 30, v: 0, gust: 40 } : null), true);
const detail: Layer = { a: d2, b: d2, t: 0 };

test.beforeEach(async ({ page }) => {
  await page.goto('/e2e/gpu.html');
  await page.waitForFunction(() => typeof window.windBoth === 'function');
});

async function both(
  page: import('@playwright/test').Page,
  layers: { base: Layer | null; mid: Layer | null; detail: Layer | null },
  places: [number, number][],
): Promise<Both> {
  return page.evaluate(([l, p]) => window.windBoth(l, p), [layers, places] as const);
}

/** 8-bit textures: a step is 80/255 kn */
const NEAR = 0.35;

function expectWind(got: { u: number; v: number; gust: number } | null, u: number, v: number, gust: number) {
  expect(got).not.toBeNull();
  expect(Math.abs((got?.u ?? 0) - u), `u ${got?.u} for ${u}`).toBeLessThan(NEAR);
  expect(Math.abs((got?.v ?? 0) - v), `v ${got?.v} for ${v}`).toBeLessThan(NEAR);
  expect(Math.abs((got?.gust ?? 0) - gust), `gust ${got?.gust} for ${gust}`).toBeLessThan(NEAR);
}

test('each model where it is finest, wind and gusts', async ({ page }) => {
  const places: [number, number][] = [
    [8, 50], // deep inside ICON-D2
    [13, 50], // beyond ICON-D2's domain, inside its texture: ICON global
    [25, 45], // ICON global's piece
    [-20, 45], // outside the piece: ECMWF
    [179.9, 0], // across 180° on ECMWF
    [-179.9, 0],
    [340, 45], // a world copy east: the same place as 20° W
  ];
  const { gpu, cpu } = await both(page, { base, mid, detail }, places);
  for (const got of [gpu, cpu]) {
    expectWind(got[0], 30, 0, 40);
    // ICON global's second hour has no gust: the first one's stays
    expectWind(got[1], 20, 5, 30);
    expectWind(got[2], 20, 5, 30);
    // ECMWF's first hour has none: the second one's
    expectWind(got[3], ecmwf(-20).u, -5, 20);
    expectWind(got[4], ecmwf(179.9).u, -5, 20);
    expectWind(got[5], ecmwf(-179.9).u, -5, 20);
    expectWind(got[6], ecmwf(-20).u, -5, 20);
  }
  expect(cpu.map((c) => c?.model)).toEqual([
    'icon_d2',
    'icon_global',
    'icon_global',
    'ecmwf_ifs025',
    'ecmwf_ifs025',
    'ecmwf_ifs025',
    'ecmwf_ifs025',
  ]);
});

test('no edge where one model gives way to the next', async ({ page }) => {
  // Across ICON-D2's edge at 12° E and ICON global's west edge at 0°, in small steps
  const across = (from: number, to: number, lat: number) =>
    Array.from({ length: 21 }, (_, i) => [from + ((to - from) * i) / 20, lat] as [number, number]);
  const d2Edge = across(11, 12.2, 50);
  const pieceEdge = across(-0.2, 2, 45);
  const { gpu, cpu } = await both(page, { base, mid, detail }, [...d2Edge, ...pieceEdge]);
  // The CPU shows what the GPU draws, within a step of the textures
  gpu.forEach((g, i) => {
    expect(g, `place ${i}`).not.toBeNull();
    expectWind(cpu[i], g?.u ?? 0, g?.v ?? 0, g?.gust ?? 0);
  });
  // From one model's value to the next without a jump: no step bigger than a fifth of the way
  for (const [run, from, to] of [
    [gpu.slice(0, 21), 30, 20],
    [gpu.slice(21), ecmwf(0).u, 20],
  ] as const) {
    const us = run.map((g) => g?.u ?? Number.NaN);
    expect(us[0]).toBeCloseTo(from, 0);
    expect(us.at(-1)).toBeCloseTo(to, 0);
    for (let i = 1; i < us.length; i++) {
      expect(Math.abs(us[i] - us[i - 1]), `step ${i}: ${us.join(', ')}`).toBeLessThan(Math.abs(to - from) / 5);
    }
  }
});

test('the coarser models alone where the finer ones are missing', async ({ page }) => {
  // Zoomed out (no piece of ICON global) and after ICON-D2's hours: ECMWF, then nothing at all
  const alone = await both(page, { base, mid: null, detail: null }, [[8, 50]]);
  for (const got of [alone.gpu, alone.cpu]) expectWind(got[0], ecmwf(8).u, -5, 20);
  // ICON-D2 without ICON global fades straight into ECMWF
  const twoOnly = await both(page, { base, mid: null, detail }, [
    [8, 50],
    [13, 50],
  ]);
  for (const got of [twoOnly.gpu, twoOnly.cpu]) {
    expectWind(got[0], 30, 0, 40);
    expectWind(got[1], ecmwf(13).u, -5, 20);
  }
  const none = await both(page, { base: null, mid: null, detail: null }, [[8, 50]]);
  expect([none.gpu[0], none.cpu[0]]).toEqual([null, null]);
});

test("a finer model's gust stays inside its own domain", async ({ page }) => {
  // ECMWF's first hour of a run has no gusts at all; ICON global's piece around the view has.
  // Outside the piece the gust is the wind there, not the gust at the piece's edge.
  const firstHour = hour('ecmwf_ifs025', [-180.5, -90.5, 179.5, 90.5], 1, (lon) => ecmwf(lon), false);
  const noGusts: Layer = { a: firstHour, b: firstHour, t: 0 };
  const { gpu, cpu } = await both(page, { base: noGusts, mid, detail }, [[-20, 45]]);
  const speed = Math.hypot(ecmwf(-20).u, -5);
  for (const got of [gpu, cpu]) expectWind(got[0], ecmwf(-20).u, -5, speed);
});

