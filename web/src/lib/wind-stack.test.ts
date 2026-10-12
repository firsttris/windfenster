import { describe, expect, it } from 'vitest';
import type { Wind } from './wind-map';
import { alphaMip, edgeOf, innerOf, layered, over, smoothstep } from './wind-stack';

const w = (u: number, gust: number | null = u + 5): Wind => ({ u, v: 0, gust });
const at = (model: string, wind: Wind) => ({ model, wind });

describe('the models laid over one another', () => {
  // A piece of ICON global: 20–40° E, 30–50° N
  const piece = { west: 20, south: 30, east: 40, north: 50 };
  const world = { west: -180.125, south: -90.125, east: 179.875, north: 90.125 };

  it('fades a piece out at its edges only', () => {
    expect(innerOf(piece, 30, 40)).toBe(1);
    // 4 % of 20° is 0.8°: from there inwards the piece is whole
    expect(innerOf(piece, 20.8, 40)).toBe(1);
    expect(innerOf(piece, 20.4, 40)).toBeCloseTo(0.5);
    expect(innerOf(piece, 20, 40)).toBe(0);
    expect(innerOf(piece, 19, 40)).toBe(0);
    expect(innerOf(piece, 30, 50.5)).toBe(0);
    // Around the world only the poles are edges, and world copies are the same places
    expect(innerOf(world, 179.9, 0)).toBe(1);
    expect(innerOf(world, 540, 0)).toBe(1);
    expect(innerOf(world, 0, 89)).toBeLessThan(1);
  });

  it('reads the detail edge from a coarse mip level', () => {
    // 64 × 32 texels, wind in the western half only
    const px = new Uint8Array(64 * 32 * 4);
    for (let y = 0; y < 32; y++) for (let x = 0; x < 32; x++) px[(y * 64 + x) * 4 + 3] = 255;
    const mip = alphaMip(px, 64, 32, 4);
    expect([mip.width, mip.height, ...mip.alpha]).toEqual([4, 2, 1, 1, 0, 0, 1, 1, 0, 0]);
    const tex = { west: 0, south: 0, east: 4, north: 2 };
    // Deep inside the part with wind, at its edge, beyond it, outside the texture
    expect(edgeOf(tex, mip, 0.5, 1)).toBe(1);
    expect(edgeOf(tex, mip, 2, 1)).toBeCloseTo(0.5);
    expect(edgeOf(tex, mip, 3.5, 1)).toBe(0);
    expect(edgeOf(tex, mip, 5, 1)).toBe(0);
  });

  it('takes the finest model inside its domain and blends at the edge', () => {
    const base = at('ecmwf', w(10));
    const mid = at('icon_global', w(20));
    const detail = at('icon_d2', w(30));
    expect(layered(base, mid, detail, 1, 1)).toEqual(detail);
    // Halfway through the detail's edge zone: halfway to what lies under it
    expect(smoothstep(0.6, 0.98, 0.79)).toBeCloseTo(0.5);
    const edge = layered(base, mid, detail, 1, 0.79);
    expect(edge?.wind.u).toBeCloseTo(25);
    // Outside the detail: ICON global, and at the piece's edge into ECMWF
    expect(layered(base, mid, detail, 1, 0)).toEqual(mid);
    expect(layered(base, mid, null, 0.5, 0)?.wind.u).toBeCloseTo(15);
    expect(layered(base, mid, null, 0, 0)).toEqual(base);
    // Without ICON global (too far out, or turned off) ICON-D2 fades into ECMWF
    expect(layered(base, null, detail, 0, 0.79)?.wind.u).toBeCloseTo(20);
    expect(layered(null, null, null, 1, 1)).toBeNull();
  });

  it('shows a model whole where nothing lies under it', () => {
    const detail = at('icon_d2', w(30));
    expect(layered(null, null, detail, 0, 0.6)).toEqual(detail);
    expect(over(null, detail, 0)).toEqual(detail);
  });

  it('keeps the gusts across models that lack them', () => {
    // ECMWF's first hour has no gust: ICON global's stays
    const blended = layered(at('ecmwf', w(10, null)), at('icon_global', w(20, 26)), null, 0.5, 0);
    expect(blended?.wind.gust).toBe(26);
    // and the other way round
    expect(layered(at('ecmwf', w(10, 14)), at('icon_global', w(20, null)), null, 0.5, 0)?.wind.gust).toBe(14);
    expect(layered(at('ecmwf', w(10, 14)), at('icon_global', w(20, 26)), null, 0.5, 0)?.wind.gust).toBe(20);
    expect(layered(at('ecmwf', w(10, null)), null, null, 0, 0)?.wind.gust).toBeNull();
  });

  it('names the model that weighs most', () => {
    const base = at('ecmwf', w(10));
    const mid = at('icon_global', w(20));
    expect(layered(base, mid, null, 0.6, 0)?.model).toBe('icon_global');
    expect(layered(base, mid, null, 0.4, 0)?.model).toBe('ecmwf');
  });
});
