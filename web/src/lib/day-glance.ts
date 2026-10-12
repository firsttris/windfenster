/** The ten days at a glance, above the forecast: per day its 3-hour bars, the main direction and
 * strongest wind over the day, and for a saved spot the hours that work there. */
import type { GearItem } from '@/bindings/GearItem';
import type { Sector } from '@/bindings/Sector';
import { inSector } from './favorites';
import { type Cell, type Day, isNight } from './forecast';
import { gearFor } from './gear';

/** Without gear a working hour has this much wind, as a windy day counts in the statistics */
export const WORKS_FROM_KN = 12;

export interface GlanceBar {
  hour: number;
  wind: number;
  gust: number;
  night: boolean;
  /** The hour is over */
  past: boolean;
  /** The hour works at the saved spot */
  works: boolean;
}

export interface DayGlance {
  date: string;
  bars: GlanceBar[];
  /** Where the day's wind comes from, weighted by its strength (degrees), over the day */
  main: number | null;
  /** The strongest wind over the day */
  max: number | null;
  /** Some hour of the day works at the spot */
  works: boolean;
}

/** Whether an hour works at a saved spot: in daylight, from its directions (any without), and
 * strong enough: for the user's gear when there is some, else from 12 kn. */
export function spotWorks(sector: Sector | null, gear: readonly GearItem[]): (c: Cell) => boolean {
  return (c) => {
    if (c.wind === null || isNight(c.hour)) return false;
    if (sector && (c.dir === null || !inSector(sector, c.dir))) return false;
    if (gear.length > 0) return typeof gearFor(gear, c.wind, c.gust) === 'object';
    return c.wind >= WORKS_FROM_KN;
  };
}

/** The main direction of some hours: the mean of their wind vectors, so strong hours count
 * more and north stays north across 360°; null in calm or without directions. */
export function mainDirection(cells: readonly Cell[]): number | null {
  let [x, y] = [0, 0];
  for (const c of cells) {
    if (c.wind === null || c.dir === null) continue;
    const r = (c.dir * Math.PI) / 180;
    x += Math.sin(r) * c.wind;
    y += Math.cos(r) * c.wind;
  }
  if (Math.hypot(x, y) < 0.5) return null;
  return ((Math.atan2(x, y) * 180) / Math.PI + 360) % 360;
}

/** A day at a glance; `works` marks the hours that work at a saved spot (none without one). */
export function dayGlance(day: Day, works: ((c: Cell) => boolean) | null = null): DayGlance {
  const daytime = day.cells.filter((c) => !isNight(c.hour) && c.wind !== null);
  const bars = day.cells.flatMap((c) =>
    c.wind === null
      ? []
      : [
          {
            hour: c.hour,
            wind: c.wind,
            gust: Math.max(c.gust ?? 0, c.wind),
            night: isNight(c.hour),
            past: c.past,
            works: !!works?.(c),
          },
        ],
  );
  return {
    date: day.date,
    bars,
    main: mainDirection(daytime),
    max: daytime.length > 0 ? Math.max(...daytime.map((c) => c.wind as number)) : null,
    works: bars.some((b) => b.works),
  };
}
