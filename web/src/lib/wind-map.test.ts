import { describe, expect, it } from 'vitest';
import type { MapFrame } from '@/bindings/MapFrame';
import type { TextureMeta } from '@/bindings/TextureMeta';
import {
  blend,
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
  // 2 × 2 texels, north row first; U from -10 to 10 kn, V from 0 to 20 kn; the north-east
  // texel lies outside the domain.
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
  };
  const px = [0, 0, 0, 255, 0, 0, 0, 0, 255, 255, 0, 255, 255, 0, 0, 255];

  it('reads texel centres and blends between them', () => {
    // Centre of the north-west texel: U -10, V 0
    expect(sampleWind(meta, px, 0.5, 51.5)).toEqual({ u: -10, v: 0 });
    // Centre of the south-west texel
    expect(sampleWind(meta, px, 0.5, 50.5)).toEqual({ u: 10, v: 20 });
    // Half way between the two western texels
    expect(sampleWind(meta, px, 0.5, 51)).toEqual({ u: 0, v: 10 });
    // Next to the missing texel only the valid ones count.
    expect(sampleWind(meta, px, 1.5, 51)).toEqual({ u: 10, v: 0 });
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
    expect(blend({ u: 0, v: 10 }, { u: 10, v: 0 }, 0.25)).toEqual({ u: 2.5, v: 7.5 });
    expect(blend(null, { u: 1, v: 1 }, 0.5)).toEqual({ u: 1, v: 1 });
  });
});
