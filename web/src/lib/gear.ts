/**
 * Which of the user's gear fits an hour: every piece (kite, wing, windsurf sail, parawing) has
 * the mean wind it works in; of those that fit, the one with the wind nearest the middle of its
 * range. The kind only names a piece. Templates fill a first list to adjust.
 */
import type { GearItem } from '@/bindings/GearItem';
import type { GearKind } from '@/bindings/GearKind';

export const GEAR_KINDS: readonly GearKind[] = ['kite', 'wing', 'windsurf', 'parawing'];

export type GearFit =
  | { fits: GearItem; gusty: boolean }
  /** Less wind than any piece needs */
  | 'low'
  /** More wind than any piece takes */
  | 'high'
  /** Between ranges: none fits, but neither all too small nor all too big */
  | 'none';

/** What fits a mean wind (and its gusts), or null without gear. */
export function gearFor(gear: readonly GearItem[], wind: number, gust: number | null): GearFit | null {
  if (gear.length === 0) return null;
  const fitting = gear.filter((g) => wind >= g.minKn && wind <= g.maxKn);
  if (fitting.length === 0) {
    if (gear.every((g) => wind < g.minKn)) return 'low';
    if (gear.every((g) => wind > g.maxKn)) return 'high';
    return 'none';
  }
  const offCentre = (g: GearItem) => Math.abs(wind - (g.minKn + g.maxKn) / 2) / (g.maxKn - g.minKn);
  const best = fitting.reduce((a, b) => (offCentre(b) < offCentre(a) ? b : a));
  return { fits: best, gusty: gust !== null && gust > best.maxKn };
}

/** "4" or "4.5": a size as riders say it */
export function sizeText(size: number): string {
  return Number.isInteger(size) ? String(size) : size.toFixed(1);
}

export type GearLevel = 'beginner' | 'intermediate' | 'expert';

/** A rider of about 80 kg at the intermediate level, per kind: size, low and high end in kn.
 * Rough starting points from common size charts, meant to be adjusted. */
const TEMPLATES: Record<GearKind, readonly [number, number, number][]> = {
  kite: [
    [14, 10, 16],
    [12, 12, 19],
    [10, 15, 23],
    [8, 19, 28],
  ],
  wing: [
    [6, 12, 18],
    [5, 14, 22],
    [4, 17, 27],
    [3, 22, 35],
  ],
  windsurf: [
    [7.5, 12, 18],
    [6.5, 14, 21],
    [5.5, 17, 25],
    [4.7, 21, 30],
  ],
  parawing: [
    [5, 14, 22],
    [4, 17, 26],
    [3, 21, 32],
  ],
};

/** Knots a level moves the ranges: beginners need more wind, experts get going with less */
const LEVEL_SHIFT: Record<GearLevel, number> = { beginner: 3, intermediate: 0, expert: -3 };

export function template(kind: GearKind, level: GearLevel): GearItem[] {
  const shift = LEVEL_SHIFT[level];
  return TEMPLATES[kind].map(([size, lo, hi]) => ({
    kind,
    size,
    minKn: Math.max(0, lo + shift),
    maxKn: hi + shift,
  }));
}
