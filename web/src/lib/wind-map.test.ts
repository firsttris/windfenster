import { describe, expect, it } from 'vitest';
import type { MapFrame } from '@/bindings/MapFrame';
import type { TextureMeta } from '@/bindings/TextureMeta';
import {
  blend,
  chooseLayers,
  gustOf,
  hoursAt,
  insideRing,
  latOf,
  mercatorX,
  mercatorY,
  positionAt,
  RAMP_MAX_KN,
  rampPixels,
  sampleWind,
  speedAndDirection,
  startTime,
  viewOf,
  wrapsAround,
} from './wind-map';

const frame = (validAt: number): MapFrame => ({ validAt, runAt: 0, path: `icon_d2/2026101106/${validAt}` });

describe('mercator', () => {
  it('maps the world to 0…1 and back', () => {
    expect(mercatorX(-180)).toBe(0);
    expect(mercatorX(8.08)).toBeCloseTo(0.52244, 5);
    expect(mercatorY(0)).toBeCloseTo(0.5, 12);
    expect(mercatorY(85.0511)).toBeCloseTo(0, 4);
    expect(latOf(mercatorY(48.78))).toBeCloseTo(48.78, 9);
    const v = viewOf(0, 40, 10, 50);
    expect(v.w).toBeCloseTo(10 / 360, 12);
    expect(v.y).toBeCloseTo(mercatorY(50), 12);
    expect(v.h).toBeGreaterThan(0);
  });
});

describe('timeline', () => {
  const frames = [frame(0), frame(3_600), frame(10_800)];

  it('finds the frames around a time', () => {
    expect(positionAt(frames, -5)).toEqual({ i: 0, t: 0 });
    expect(positionAt(frames, 1_800)).toEqual({ i: 0, t: 0.5 });
    expect(positionAt(frames, 3_600)).toEqual({ i: 1, t: 0 });
    // A gap of two hours is crossed in proportion.
    expect(positionAt(frames, 5_400)).toEqual({ i: 1, t: 0.25 });
    expect(positionAt(frames, 99_999)).toEqual({ i: 2, t: 0 });
    expect(positionAt([], 5)).toEqual({ i: 0, t: 0 });
  });

  it('opens at the hour nearest to now', () => {
    expect(startTime(frames, 2_000)).toBe(3_600);
    expect(startTime(frames, 99_999)).toBe(10_800);
    expect(startTime([], 0)).toBeNull();
  });
});

describe('colour ramp', () => {
  it('runs through the wind scale and holds its last colour', () => {
    const px = rampPixels(RAMP_MAX_KN + 1);
    // 1 kn per texel: 0 kn is the calm colour #1c2433, 5 kn the next band's #1e4a6b.
    expect([...px.slice(0, 4)]).toEqual([0x1c, 0x24, 0x33, 255]);
    expect([...px.slice(5 * 4, 5 * 4 + 3)]).toEqual([0x1e, 0x4a, 0x6b]);
    expect([...px.slice(40 * 4, 40 * 4 + 3)]).toEqual([0xb2, 0x3a, 0x8f]);
  });
});

describe('sampling a texture', () => {
  // 2 × 2 texels, north row first; U from -10 to 10 kn, V from 0 to 20 kn, gusts up to 30 kn;
  // the north-east texel lies outside the domain.
  const meta: TextureMeta = {
    model: 'icon_d2',
    runAt: 0,
    step: 0,
    validAt: 0,
    west: 0,
    south: 50,
    east: 2,
    north: 52,
    width: 2,
    height: 2,
    domain: [],
    uMin: -10,
    uMax: 10,
    vMin: 0,
    vMax: 20,
    gustMax: 30,
  };
  const px = [0, 0, 0, 255, 0, 0, 0, 0, 255, 255, 255, 255, 255, 0, 102, 255];

  it('reads texel centres and blends between them', () => {
    // Centre of the north-west texel: U -10, V 0
    expect(sampleWind(meta, px, 0.5, 51.5)).toEqual({ u: -10, v: 0, gust: 0 });
    // Centre of the south-west texel
    expect(sampleWind(meta, px, 0.5, 50.5)).toEqual({ u: 10, v: 20, gust: 30 });
    // Half way between the two western texels
    expect(sampleWind(meta, px, 0.5, 51)).toEqual({ u: 0, v: 10, gust: 15 });
    // Next to the missing texel only the valid ones count.
    expect(sampleWind(meta, px, 1.5, 51)).toEqual({ u: 10, v: 0, gust: 12 });
  });

  it('wraps a global texture at 180°, also on world copies', () => {
    // Two columns of 180°: west half U -10, east half U 10 (north row), centres at -90 and 90
    const world = { ...meta, west: -180, east: 180, south: -90, north: 90 };
    expect(wrapsAround(world)).toBe(true);
    expect(wrapsAround(meta)).toBe(false);
    // At 180° both columns count half each, the same 360° further.
    expect(sampleWind(world, px, 180, 45)).toEqual({ u: -10, v: 0, gust: 0 });
    expect(sampleWind(world, px, 0, 45)).toEqual({ u: -10, v: 0, gust: 0 });
    expect(sampleWind(world, px, -90, 45)).toEqual(sampleWind(world, px, 270, 45));
    expect(sampleWind(world, px, -90, 45)).toEqual(sampleWind(world, px, -450, 45));
    // A regional texture on a world copy
    expect(sampleWind(meta, px, 360.5, 51.5)).toEqual({ u: -10, v: 0, gust: 0 });
  });

  it('has no gusts where the model hour has none', () => {
    expect(sampleWind({ ...meta, gustMax: null }, px, 0.5, 50.5)?.gust).toBeNull();
  });

  it('knows its domain', () => {
    expect(sampleWind(meta, px, 1.5, 51.5)).toBeNull();
    expect(sampleWind(meta, px, -1, 51)).toBeNull();
  });
});

describe('wind', () => {
  it('gives speed and where it comes from', () => {
    // Blowing towards the south: from the north
    expect(speedAndDirection({ u: 0, v: -5 })).toEqual({ kn: 5, from: 0 });
    // Blowing towards the north-east: from the south-west
    const sw = speedAndDirection({ u: 3, v: 3 });
    expect(sw.from).toBeCloseTo(225, 9);
    expect(sw.kn).toBeCloseTo(Math.hypot(3, 3), 9);
  });

  it('blends two hours', () => {
    expect(blend({ u: 0, v: 10, gust: 20 }, { u: 10, v: 0, gust: 40 }, 0.25)).toEqual({ u: 2.5, v: 7.5, gust: 25 });
    expect(blend(null, { u: 1, v: 1, gust: null }, 0.5)).toEqual({ u: 1, v: 1, gust: null });
    // The first hour of a run has no gusts: the next hour's stand in.
    expect(blend({ u: 0, v: 0, gust: null }, { u: 0, v: 0, gust: 18 }, 0.5)?.gust).toBe(18);
  });

  it('never shows gusts below the wind', () => {
    expect(gustOf({ u: 6, v: 8, gust: 14 })).toBe(14);
    expect(gustOf({ u: 6, v: 8, gust: 9 })).toBe(10);
    expect(gustOf({ u: 6, v: 8, gust: null })).toBe(10);
  });
});

describe('domains', () => {
  it('knows what lies within a ring', () => {
    // ICON-D2's domain, roughly, and a notch cut out in the south-east
    const ring: [number, number][] = [
      [-4, 44],
      [10, 44],
      [10, 50],
      [20, 50],
      [20, 59],
      [-4, 59],
    ];
    expect(insideRing(ring, 8.08, 48.78)).toBe(true);
    expect(insideRing(ring, 28.09, 36.41)).toBe(false);
    expect(insideRing(ring, 15, 47)).toBe(false);
    expect(insideRing(ring, 15, 52)).toBe(true);
    expect(insideRing([], 0, 0)).toBe(false);
  });
});

describe('layers', () => {
  const at = (model: string, hours: number[]) => ({
    model,
    frames: hours.map((h) => ({ validAt: h * 3_600, runAt: 0, path: `${model}/2026101100/${h}` })),
  });
  const icon = at('icon_d2', [0, 1, 2]);
  const ecmwf = at('ecmwf_ifs025', [0, 3, 6]);

  it('puts the finer model over the base', () => {
    const c = chooseLayers([icon, ecmwf], null);
    expect([c.selected?.model, c.detail?.model, c.base?.model]).toEqual(['icon_d2', 'icon_d2', 'ecmwf_ifs025']);
    // Each hour once, the finer model's first
    expect(c.frames.map((f) => f.path)).toEqual([
      'icon_d2/2026101100/0',
      'icon_d2/2026101100/1',
      'icon_d2/2026101100/2',
      'ecmwf_ifs025/2026101100/3',
      'ecmwf_ifs025/2026101100/6',
    ]);
  });

  it('shows the base alone when picked, and whatever has hours', () => {
    const c = chooseLayers([icon, ecmwf], 'ecmwf_ifs025');
    expect([c.detail, c.base?.model]).toEqual([null, 'ecmwf_ifs025']);
    const d = chooseLayers([icon, at('ecmwf_ifs025', [])], null);
    expect([d.detail?.model, d.base]).toEqual(['icon_d2', null]);
    expect(chooseLayers([], null).selected).toBeNull();
  });

  it('ends the finer model with its hours', () => {
    expect(hoursAt(icon.frames, 3_600)).toBe(icon.frames);
    expect(hoursAt(icon.frames, 4 * 3_600)).toEqual([]);
    expect(hoursAt(icon.frames, null)).toEqual([]);
  });
});
