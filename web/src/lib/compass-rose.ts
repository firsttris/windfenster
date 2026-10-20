/**
 * Geometry of the wind compass in "Jetzt gemessen": points and arcs on circles, degrees
 * clockwise from north as the wind's direction is given, and the METAR's variation range.
 */
import { decodeMetar } from './metar';

/** The point at `deg` on a circle of radius `r` around (`c`, `c`) */
export function xy(deg: number, r: number, c: number): [number, number] {
  const a = ((deg - 90) * Math.PI) / 180;
  const round = (v: number) => Math.round(v * 100) / 100;
  return [round(c + r * Math.cos(a)), round(c + r * Math.sin(a))];
}

/** The same as "x y", for a path */
export function at(deg: number, r: number, c: number): string {
  return xy(deg, r, c).join(' ');
}

/** An arc along the circle clockwise from `from` to `to` (an SVG path, open) */
export function arc(from: number, to: number, r: number, c: number): string {
  const span = (((to - from) % 360) + 360) % 360;
  // A full circle would start and end on the same point and draw nothing
  if (span === 0)
    return `M${at(from, r, c)} A${r} ${r} 0 1 1 ${at(from + 180, r, c)} A${r} ${r} 0 1 1 ${at(from, r, c)}`;
  return `M${at(from, r, c)} A${r} ${r} 0 ${span > 180 ? 1 : 0} 1 ${at(to, r, c)}`;
}

/** The range the wind varied in, from the METAR's `dddVddd` group; null without one */
export function variation(raw: string | null): { from: number; to: number } | null {
  if (!raw) return null;
  for (const g of decodeMetar(raw)) if (g.kind === 'windVariation') return { from: g.from, to: g.to };
  return null;
}
