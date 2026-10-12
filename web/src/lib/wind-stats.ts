/**
 * A station's wind statistics as the page shows them (`/api/stations/{slug}/stats`): a month or
 * the whole year, its rose as shares of the hours and its windy days.
 */
import type { MonthStats } from '@/bindings/MonthStats';
import type { WindStats } from '@/bindings/WindStats';

/** The months added up: the year */
export function wholeYear(stats: WindStats): MonthStats {
  const sum = (f: (m: MonthStats) => number) => stats.months.reduce((s, m) => s + f(m), 0);
  return {
    hours: sum((m) => m.hours),
    calm: sum((m) => m.calm),
    days: sum((m) => m.days),
    windyDays: sum((m) => m.windyDays),
    rose: stats.months[0].rose.map((bands, sector) => bands.map((_, band) => sum((m) => m.rose[sector][band]))),
  };
}

export interface Rose {
  /** Per sector (16, north first, clockwise) the share of all hours per speed band, 0…1 */
  sectors: number[][];
  /** Share of all hours of the fullest sector */
  max: number;
  /** Share of all hours calm or of no direction */
  calm: number;
  /** The sector most wind comes from, or null without any */
  main: number | null;
}

export function rose(m: MonthStats): Rose {
  const hours = Math.max(1, m.hours);
  const sectors = m.rose.map((bands) => bands.map((n) => n / hours));
  const totals = sectors.map((bands) => bands.reduce((s, x) => s + x, 0));
  const max = Math.max(0, ...totals);
  return {
    sectors,
    max,
    calm: m.calm / hours,
    main: max > 0 ? totals.indexOf(max) : null,
  };
}

/** The share of windy days, 0…1; null without days measured */
export function windyShare(m: MonthStats): number | null {
  return m.days > 0 ? m.windyDays / m.days : null;
}

/** The direction in the middle of a sector, degrees */
export const sectorDegrees = (sector: number) => sector * 22.5;

/** The rings of the rose: shares of the hours in even steps, the last one at or beyond `max`
 * so the fullest petal ends inside it */
export function rings(max: number): number[] {
  const step = max > 0.3 ? 0.1 : max > 0.12 ? 0.05 : max > 0.04 ? 0.02 : 0.01;
  const out: number[] = [];
  for (let r = step; out.length === 0 || out[out.length - 1] < max - 1e-9; r += step) {
    out.push(Math.round(r * 1000) / 1000);
  }
  return out;
}

/** One petal: per speed band the inner and outer share (of `max`), stacked outwards from the
 * centre, weakest inside */
export function petal(bands: number[], max: number): { band: number; from: number; to: number }[] {
  let at = 0;
  return bands.flatMap((share, band) => {
    if (share <= 0 || max <= 0) return [];
    const from = at / max;
    at += share;
    return [{ band, from, to: at / max }];
  });
}
