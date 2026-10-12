/**
 * How the map lays its models over one another, on the CPU: the same rules as `wind()` in
 * `components/map/shaders.ts`, so the readout under the pointer, the spot card and the arrows
 * show what the colour shows. Change both together.
 *
 * Up to three models, finest on top: the base (ECMWF) everywhere; over it a finer global model
 * (ICON global) in the piece the app has, fading out over the piece's outer 4 %; over both the
 * detail (ICON-D2) inside its domain, fading over its last 30–60 km to the edge, where the share
 * of its texels with wind, read from mip level 4, drops from 0.98 to 0.6.
 */
import type { TextureMeta } from '@/bindings/TextureMeta';
import { blend, type Wind, wrapsAround } from './wind-map';

/** The mip level the detail's edge is read from, as `EDGE_LOD` in the shaders */
export const EDGE_LOD = 4;

export function smoothstep(e0: number, e1: number, x: number): number {
  const t = Math.min(1, Math.max(0, (x - e0) / (e1 - e0)));
  return t * t * (3 - 2 * t);
}

/** A place in a texture's coordinates: 0…1 west to east (any value around the world) and north
 * to south */
function uvOf(meta: Pick<TextureMeta, 'west' | 'south' | 'east' | 'north'>, lon: number, lat: number) {
  const span = meta.east - meta.west;
  const at = wrapsAround(meta) ? lon : ((((lon + 180) % 360) + 360) % 360) - 180;
  return { x: (at - meta.west) / span, y: (meta.north - lat) / (meta.north - meta.south) };
}

/** How far inside a piece: 0 at its edges and outside, 1 from 4 % of its size inwards (only along
 * the latitude for a texture around the world). */
export function innerOf(
  meta: Pick<TextureMeta, 'west' | 'south' | 'east' | 'north'>,
  lon: number,
  lat: number,
): number {
  const { x, y } = uvOf(meta, lon, lat);
  const wraps = wrapsAround(meta);
  if (y < 0 || y > 1 || (!wraps && (x < 0 || x > 1))) return 0;
  const d = wraps ? Math.min(y, 1 - y) : Math.min(x, 1 - x, y, 1 - y);
  return smoothstep(0, 0.04, d);
}

/** The alpha of a texture at a mip level, as `generateMipmap` makes it in Chromium (ANGLE): level
 * by level, sizes halved and rounded down, each texel read bilinearly from the level above at
 * its own centre, so an odd size is spread over the whole width. Rows north first, as the PNG. */
export function alphaMip(
  pixels: ArrayLike<number>,
  width: number,
  height: number,
  level: number,
): { alpha: Float32Array; width: number; height: number } {
  let w = width;
  let h = height;
  let alpha = Float32Array.from({ length: w * h }, (_, k) => pixels[k * 4 + 3] / 255);
  for (let l = 0; l < level; l++) {
    const [nw, nh] = [Math.max(1, Math.floor(w / 2)), Math.max(1, Math.floor(h / 2))];
    const next = new Float32Array(nw * nh);
    const [from, fw, fh] = [alpha, w, h];
    const at = (x: number, y: number) => from[Math.min(fh - 1, Math.max(0, y)) * fw + Math.min(fw - 1, Math.max(0, x))];
    for (let j = 0; j < nh; j++) {
      const fy = ((j + 0.5) * fh) / nh - 0.5;
      const y0 = Math.floor(fy);
      const ay = fy - y0;
      for (let i = 0; i < nw; i++) {
        const fx = ((i + 0.5) * fw) / nw - 0.5;
        const x0 = Math.floor(fx);
        const ax = fx - x0;
        next[j * nw + i] =
          at(x0, y0) * (1 - ax) * (1 - ay) +
          at(x0 + 1, y0) * ax * (1 - ay) +
          at(x0, y0 + 1) * (1 - ax) * ay +
          at(x0 + 1, y0 + 1) * ax * ay;
      }
    }
    [alpha, w, h] = [next, nw, nh];
  }
  return { alpha, width: w, height: h };
}

/** The share of the detail's texels with wind around a place, interpolated in its mip level as
 * the GPU does (edges clamped); 0 outside its texture. */
export function edgeOf(
  meta: Pick<TextureMeta, 'west' | 'south' | 'east' | 'north'>,
  mip: ReturnType<typeof alphaMip>,
  lon: number,
  lat: number,
): number {
  const { x, y } = uvOf(meta, lon, lat);
  if (x < 0 || x > 1 || y < 0 || y > 1) return 0;
  const fx = Math.min(mip.width - 1, Math.max(0, x * mip.width - 0.5));
  const fy = Math.min(mip.height - 1, Math.max(0, y * mip.height - 0.5));
  const [x0, y0] = [Math.floor(fx), Math.floor(fy)];
  const [x1, y1] = [Math.min(mip.width - 1, x0 + 1), Math.min(mip.height - 1, y0 + 1)];
  const [ax, ay] = [fx - x0, fy - y0];
  const at = (i: number, j: number) => mip.alpha[j * mip.width + i];
  return (
    at(x0, y0) * (1 - ax) * (1 - ay) + at(x1, y0) * ax * (1 - ay) + at(x0, y1) * (1 - ax) * ay + at(x1, y1) * ax * ay
  );
}

/** One layer over another, `w` of the way to the upper where it has wind; the whole upper where
 * nothing lies under it. A gust missing on one side is taken from the other. */
export function over<M>(
  under: { wind: Wind; model: M } | null,
  top: { wind: Wind; model: M } | null,
  w: number,
): { wind: Wind; model: M } | null {
  if (!top) return under;
  if (!under) return top;
  const wind = blend(under.wind, top.wind, w) as Wind;
  return { wind, model: w >= 0.5 ? top.model : under.model };
}

/** The wind the map shows: each model's wind at the place (its two hours already blended; null
 * where it has none), the mid model's `innerOf` and the detail's `edgeOf` there. The model is
 * the one that weighs most. */
export function layered<M>(
  base: { wind: Wind; model: M } | null,
  mid: { wind: Wind; model: M } | null,
  detail: { wind: Wind; model: M } | null,
  midInner: number,
  detailEdge: number,
): { wind: Wind; model: M } | null {
  return over(over(base, mid, midInner), detail, smoothstep(0.6, 0.98, detailEdge));
}
