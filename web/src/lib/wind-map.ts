/**
 * The wind map without the map: Web Mercator, the timeline over the model hours, the colour
 * ramp of the field and reading a texture on the CPU, as the shaders do on the GPU.
 */
import type { MapFrame } from '@/bindings/MapFrame';
import type { TextureMeta } from '@/bindings/TextureMeta';
import { WIND_SCALE } from '@/ui/wind-scale';

/** Web Mercator x, 0 at 180° W and 1 at 180° E, as MapLibre's custom layers take it. */
export function mercatorX(lon: number): number {
  return (lon + 180) / 360;
}

/** Web Mercator y, 0 at the northern edge (85.05° N) and 1 at the southern. */
export function mercatorY(lat: number): number {
  const s = Math.sin((lat * Math.PI) / 180);
  return 0.5 - Math.log((1 + s) / (1 - s)) / (4 * Math.PI);
}

export function latOf(y: number): number {
  return (Math.atan(Math.sinh(Math.PI * (1 - 2 * y))) * 180) / Math.PI;
}

/** The visible map as a Mercator rectangle: left, top, width, height. */
export interface MercatorView {
  x: number;
  y: number;
  w: number;
  h: number;
}

export function viewOf(west: number, south: number, east: number, north: number): MercatorView {
  const x = mercatorX(west);
  const y = mercatorY(north);
  return { x, y, w: mercatorX(east) - x, h: mercatorY(south) - y };
}

/** A moment on the timeline: between frame `i` and the next, `t` of the way (0…1). */
export interface FramePosition {
  i: number;
  t: number;
}

/** Where `time` falls among the frames (ascending), clamped to the first and the last. */
export function positionAt(frames: readonly MapFrame[], time: number): FramePosition {
  if (frames.length === 0 || time <= frames[0].validAt) return { i: 0, t: 0 };
  const last = frames.length - 1;
  if (time >= frames[last].validAt) return { i: last, t: 0 };
  let i = 0;
  while (frames[i + 1].validAt <= time) i++;
  const span = frames[i + 1].validAt - frames[i].validAt;
  return { i, t: (time - frames[i].validAt) / span };
}

/** The hour to open with: the frame nearest to now. */
export function startTime(frames: readonly MapFrame[], now: number): number | null {
  if (frames.length === 0) return null;
  return frames.reduce(
    (best, f) => (Math.abs(f.validAt - now) < Math.abs(best - now) ? f.validAt : best),
    frames[0].validAt,
  );
}

/** Knots at the right end of the colour ramp; stronger wind keeps the last colour. */
export const RAMP_MAX_KN = 40;

function rgb(hex: string): [number, number, number] {
  const n = Number.parseInt(hex.slice(1), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

/** The field's colours, `width` steps from 0 to RAMP_MAX_KN: the wind scale's bands, blended
 * between their lower edges. RGBA, for a width × 1 texture. */
export function rampPixels(width = 256): Uint8Array {
  const stops = WIND_SCALE.map((b) => ({ kn: b.from, c: rgb(b.bg) }));
  const out = new Uint8Array(width * 4);
  for (let x = 0; x < width; x++) {
    const kn = (x / (width - 1)) * RAMP_MAX_KN;
    let k = 0;
    while (k < stops.length - 1 && stops[k + 1].kn <= kn) k++;
    const a = stops[k];
    const b = stops[Math.min(k + 1, stops.length - 1)];
    const f = b.kn > a.kn ? Math.min(1, (kn - a.kn) / (b.kn - a.kn)) : 0;
    for (let ch = 0; ch < 3; ch++) out[x * 4 + ch] = Math.round(a.c[ch] + (b.c[ch] - a.c[ch]) * f);
    out[x * 4 + 3] = 255;
  }
  return out;
}

/** CSS gradient of the same ramp, for the legend. */
export function rampGradient(): string {
  const stops = WIND_SCALE.map((b) => `${b.bg} ${((b.from / RAMP_MAX_KN) * 100).toFixed(1)}%`);
  return `linear-gradient(to right, ${stops.join(', ')})`;
}

export interface Wind {
  /** Towards the east and the north, kn */
  u: number;
  v: number;
}

/** The wind of a texture at a place, interpolated between the four texels around it like the
 * GPU does; null outside the model's domain. `pixels` is the PNG as RGBA, north row first. */
export function sampleWind(meta: TextureMeta, pixels: ArrayLike<number>, lon: number, lat: number): Wind | null {
  const fx = ((lon - meta.west) / (meta.east - meta.west)) * meta.width - 0.5;
  const fy = ((meta.north - lat) / (meta.north - meta.south)) * meta.height - 0.5;
  if (fx < -0.5 || fy < -0.5 || fx > meta.width - 0.5 || fy > meta.height - 0.5) return null;
  const x0 = Math.max(0, Math.floor(fx));
  const y0 = Math.max(0, Math.floor(fy));
  const x1 = Math.min(meta.width - 1, x0 + 1);
  const y1 = Math.min(meta.height - 1, y0 + 1);
  const ax = Math.min(1, Math.max(0, fx - x0));
  const ay = Math.min(1, Math.max(0, fy - y0));
  let r = 0;
  let g = 0;
  let a = 0;
  for (const [x, y, w] of [
    [x0, y0, (1 - ax) * (1 - ay)],
    [x1, y0, ax * (1 - ay)],
    [x0, y1, (1 - ax) * ay],
    [x1, y1, ax * ay],
  ] as const) {
    const o = (y * meta.width + x) * 4;
    if (pixels[o + 3] === 0) continue;
    r += pixels[o] * w;
    g += pixels[o + 1] * w;
    a += w;
  }
  // Less than half of the surrounding texels in the domain counts as outside, as on the GPU.
  if (a < 0.5) return null;
  return {
    u: meta.uMin + (meta.uMax - meta.uMin) * (r / a / 255),
    v: meta.vMin + (meta.vMax - meta.vMin) * (g / a / 255),
  };
}

/** Speed in kn and where the wind comes from, degrees (0 = north). */
export function speedAndDirection({ u, v }: Wind): { kn: number; from: number } {
  const from = ((Math.atan2(-u, -v) * 180) / Math.PI + 360) % 360;
  return { kn: Math.hypot(u, v), from };
}

/** Wind of two frames blended: `t` of the way from `a` to `b`. */
export function blend(a: Wind | null, b: Wind | null, t: number): Wind | null {
  if (!a || !b) return a ?? b;
  return { u: a.u + (b.u - a.u) * t, v: a.v + (b.v - a.v) * t };
}
