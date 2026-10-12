/** Wind textures for the browser tests, encoded as the server writes them
 * (`crates/wf-map/src/texture.rs`): red U and green V over their ranges, blue the gust over 0 to
 * its maximum, alpha 0 outside the model's domain. */
import type { Hour } from './gpu';

export const RANGE = { uMin: -40, uMax: 40, vMin: -40, vMax: 40 };
export const GUST_MAX = 60;

export type Value = { u: number; v: number; gust: number | null } | null;

/** A texture of `step`° texels between the edges given, with `at(lon, lat)` at each texel's
 * centre (null: outside the model's domain), encoded as the server does (texture.rs). */
export function hour(
  model: string,
  [west, south, east, north]: [number, number, number, number],
  step: number,
  at: (lon: number, lat: number) => Value,
  gusts: boolean,
): Hour {
  const width = Math.round((east - west) / step);
  const height = Math.round((north - south) / step);
  const pixels: number[] = [];
  const byte = (x: number, min: number, max: number) => Math.round(((x - min) / (max - min)) * 255);
  for (let j = 0; j < height; j++) {
    for (let i = 0; i < width; i++) {
      const w = at(west + (i + 0.5) * step, north - (j + 0.5) * step);
      if (!w) pixels.push(0, 0, 0, 0);
      else
        pixels.push(
          byte(w.u, RANGE.uMin, RANGE.uMax),
          byte(w.v, RANGE.vMin, RANGE.vMax),
          gusts && w.gust !== null ? byte(w.gust, 0, GUST_MAX) : 0,
          255,
        );
    }
  }
  return {
    meta: {
      model,
      runAt: 0,
      step: 0,
      validAt: 0,
      west,
      south,
      east,
      north,
      width,
      height,
      domain: [],
      ...RANGE,
      gustMax: gusts ? GUST_MAX : null,
    },
    pixels,
  };
}
