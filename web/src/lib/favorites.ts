/**
 * Favourites without the UI: which favourite a page is, the compass points for the wind
 * directions that work at a spot, and whether a wind falls in them.
 */
import type { Favorite } from '@/bindings/Favorite';
import type { Sector } from '@/bindings/Sector';

/** A station page or a point page */
export type FavoriteTarget = { slug: string } | { lat: number; lon: number };

/** The 0.01° grid the server keeps points on */
const grid = (x: number) => Math.round(x * 100) / 100;

/** The favourite a page shows, if it is one. */
export function findFavorite(list: readonly Favorite[], t: FavoriteTarget): Favorite | undefined {
  if ('slug' in t) return list.find((f) => f.slug === t.slug);
  return list.find((f) => f.slug === null && f.lat === grid(t.lat) && f.lon === grid(t.lon));
}

/** The 16 compass points as degrees, 22.5° apart from north (names: `m.compass_points`) */
export const COMPASS_DEGREES: readonly number[] = Array.from({ length: 16 }, (_, i) => Math.round(i * 22.5));

/** Whether wind from `deg` falls in the sector, clockwise from `from` to `to`. */
export function inSector(s: Sector, deg: number): boolean {
  const d = ((deg % 360) + 360) % 360;
  return s.from <= s.to ? d >= s.from && d <= s.to : d >= s.from || d <= s.to;
}
