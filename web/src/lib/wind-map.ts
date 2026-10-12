/**
 * The wind map without the map: Web Mercator, the timeline over the model hours, the colour
 * ramp of the field and reading a texture on the CPU, as the shaders do on the GPU.
 */
import type { MapFrame } from '@/bindings/MapFrame';
import type { MapLayer } from '@/bindings/MapLayer';
import type { TextureMeta } from '@/bindings/TextureMeta';
import { WIND_SCALE } from '@/ui/wind-scale';

/** Web Mercator x, 0 at 180° W and 1 at 180° E, as MapLibre's custom layers take it. */
export function mercatorX(lon: number): number {
  return (lon + 180) / 360;
}

/** Web Mercator's edge, degrees north and south */
export const MAX_LAT = 85.051129;

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
  /** The strongest gust around the hour, kn; null when the model hour has none */
  gust: number | null;
}

/** Whether a texture goes once around the world, so it wraps at its west and east edges. */
export function wrapsAround(meta: Pick<TextureMeta, 'west' | 'east'>): boolean {
  return Math.abs(meta.east - meta.west - 360) < 1e-6;
}

/** The four texels around a fractional column: the left one, the right one and how far
 * towards it. A global texture wraps (left of the first texel's centre lies the last). */
function columns(fx: number, width: number, wraps: boolean): [number, number, number] {
  const f = Math.floor(fx);
  if (wraps) {
    const x0 = ((f % width) + width) % width;
    return [x0, (x0 + 1) % width, fx - f];
  }
  const x0 = Math.max(0, f);
  return [x0, Math.min(width - 1, x0 + 1), Math.min(1, Math.max(0, fx - x0))];
}

/** The wind of a texture at a place, interpolated between the four texels around it like the
 * GPU does; null outside the model's domain. `pixels` is the PNG as RGBA, north row first.
 * Longitudes beyond ±180° (MapLibre's world copies) are the same places. */
export function sampleWind(meta: TextureMeta, pixels: ArrayLike<number>, lon: number, lat: number): Wind | null {
  const wraps = wrapsAround(meta);
  const span = meta.east - meta.west;
  const at = wraps
    ? meta.west + ((((lon - meta.west) % span) + span) % span)
    : ((((lon + 180) % 360) + 360) % 360) - 180;
  const fx = ((at - meta.west) / span) * meta.width - 0.5;
  const fy = ((meta.north - lat) / (meta.north - meta.south)) * meta.height - 0.5;
  if ((!wraps && (fx < -0.5 || fx > meta.width - 0.5)) || fy < -0.5 || fy > meta.height - 0.5) return null;
  const [x0, x1, ax] = columns(fx, meta.width, wraps);
  const y0 = Math.max(0, Math.floor(fy));
  const y1 = Math.min(meta.height - 1, y0 + 1);
  const ay = Math.min(1, Math.max(0, fy - y0));
  let r = 0;
  let g = 0;
  let b = 0;
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
    b += pixels[o + 2] * w;
    a += w;
  }
  // Less than half of the surrounding texels in the domain counts as outside, as on the GPU.
  if (a < 0.5) return null;
  return {
    u: meta.uMin + (meta.uMax - meta.uMin) * (r / a / 255),
    v: meta.vMin + (meta.vMax - meta.vMin) * (g / a / 255),
    gust: meta.gustMax === null ? null : (meta.gustMax * b) / a / 255,
  };
}

/** Speed in kn and where the wind comes from, degrees (0 = north). */
export function speedAndDirection({ u, v }: Pick<Wind, 'u' | 'v'>): { kn: number; from: number } {
  const from = ((Math.atan2(-u, -v) * 180) / Math.PI + 360) % 360;
  return { kn: Math.hypot(u, v), from };
}

/** Wind of two frames blended: `t` of the way from `a` to `b`. A gust missing in one hour is
 * taken from the other. */
export function blend(a: Wind | null, b: Wind | null, t: number): Wind | null {
  if (!a || !b) return a ?? b;
  const ga = a.gust ?? b.gust;
  const gb = b.gust ?? a.gust;
  return {
    u: a.u + (b.u - a.u) * t,
    v: a.v + (b.v - a.v) * t,
    gust: ga === null || gb === null ? null : ga + (gb - ga) * t,
  };
}

/** The gust to show: the model's, never below the wind itself (both are interpolated apart),
 * or the wind where the model has no gusts. */
export function gustOf(w: Wind): number {
  return Math.max(w.gust ?? 0, Math.hypot(w.u, w.v));
}

/** Whether a place lies within a ring of (lon, lat) points, such as a model's domain. */
export function insideRing(ring: readonly (readonly [number, number])[], lon: number, lat: number): boolean {
  let inside = false;
  for (let i = 0, j = ring.length - 1; i < ring.length; j = i++) {
    const [xi, yi] = ring[i];
    const [xj, yj] = ring[j];
    if (yi > lat !== yj > lat && lon < ((xj - xi) * (lat - yi)) / (yj - yi) + xi) inside = !inside;
  }
  return inside;
}

/** Models that cover the whole map region: the base under a finer model */
export const BASE_MODELS: readonly string[] = ['ecmwf_ifs025'];

export interface MapChoice {
  /** The model picked (or the finest with hours) */
  selected: MapLayer | null;
  /** Drawn everywhere */
  base: MapLayer | null;
  /** Drawn over the base inside its domain, while its hours last */
  detail: MapLayer | null;
  /** The hours of both, for the timeline */
  frames: MapFrame[];
}

/** A place: where the map's middle is, or a spot */
export interface Place {
  lon: number;
  lat: number;
}

function inArea(area: readonly number[] | null, at: Place): boolean {
  if (!area) return false;
  const [w, s, e, n] = area;
  return at.lat >= s && at.lat <= n && (e - w >= 359 || (at.lon >= w && at.lon <= e));
}

/** What the map draws for the model picked: a finer model over the base, the base alone, or
 * whichever has hours. Nothing picked: the finest model whose area holds `at` (ICON-D2 over
 * Germany, ICON global elsewhere). */
export function chooseLayers(layers: readonly MapLayer[], chosen: string | null, at: Place | null = null): MapChoice {
  const ready = layers.filter((l) => l.frames.length > 0);
  const fine = ready.filter((l) => !BASE_MODELS.includes(l.model));
  const auto = (at && fine.find((l) => inArea(l.area, at))) ?? fine[0] ?? ready[0] ?? null;
  const selected = ready.find((l) => l.model === chosen) ?? auto;
  const base = ready.find((l) => BASE_MODELS.includes(l.model)) ?? null;
  const detail = selected && selected !== base ? selected : null;
  const seen = new Set<number>();
  const frames = [...(detail?.frames ?? []), ...(base?.frames ?? [])]
    .filter((f) => !seen.has(f.validAt) && seen.add(f.validAt))
    .sort((a, b) => a.validAt - b.validAt);
  return { selected, base, detail, frames };
}

/** The piece of a world texture around a view (west, south, east, north), as the server cuts
 * it (`crates/wf-map/src/crop.rs`): a quarter more on every side, edges on whole 10°, within
 * ±180° and ±80°; null when that is too large (from afar the base model draws alone). */
export function pieceFor([w, s, e, n]: readonly [number, number, number, number]): string | null {
  const [mx, my] = [(e - w) / 4, (n - s) / 4];
  const west = Math.max(-180, Math.floor((w - mx) / 10) * 10);
  const east = Math.min(180, Math.ceil((e + mx) / 10) * 10);
  const south = Math.max(-80, Math.floor((s - my) / 10) * 10);
  const north = Math.min(80, Math.ceil((n + my) / 10) * 10);
  if (east <= west || north <= south || east - west > 100 || north - south > 80) return null;
  return `${west}_${south}_${east}_${north}`;
}

/** A layer's hours as the map loads them: pieces around the view for a model sent in pieces,
 * none when the view is too wide for one. */
export function framesFor(layer: MapLayer | null, piece: string | null): readonly MapFrame[] {
  if (!layer) return [];
  if (!layer.inPieces) return layer.frames;
  return piece ? layer.frames.map((f) => ({ ...f, path: `${f.path}/${piece}` })) : [];
}

/** Whether a texture holds a place: inside its model's domain, or its edges for a piece */
export function covers(meta: TextureMeta, at: Place): boolean {
  if (meta.domain.length > 2) return insideRing(meta.domain as [number, number][], at.lon, at.lat);
  return at.lat >= meta.south && at.lat <= meta.north && at.lon >= meta.west && at.lon <= meta.east;
}

/** Models on the map that the forecast does not have, with what the model list says of them */
export const MAP_ONLY_MODELS: Record<string, { name: string; provider: string; resolutionKm: number }> = {
  icon_global: { name: 'ICON global', provider: 'DWD', resolutionKm: 13 },
};

/** A model's hours if `time` lies within them, else none: a finer model ends where its forecast
 * does instead of standing still at its last hour. */
export function hoursAt(frames: readonly MapFrame[], time: number | null): readonly MapFrame[] {
  const first = frames[0]?.validAt;
  const last = frames.at(-1)?.validAt;
  return time !== null && first !== undefined && last !== undefined && time >= first && time <= last ? frames : [];
}
