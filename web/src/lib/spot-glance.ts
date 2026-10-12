/**
 * A spot at a glance, for its card on the start page: its coming hours as one band in the wind
 * scale's colours (hours whose wind comes from outside the spot's directions faded), and the
 * strongest hour. Nothing to set: no threshold, no minimum length.
 */
import type { Sector } from '@/bindings/Sector';
import { windBand } from '@/ui/wind-scale';
import { inSector } from './favorites';
import type { SpotHour } from './spot-series';

/** How strongly an hour from the wrong direction shows, percent */
const FADED = 28;

/** Whether the hour's wind works at the spot: always without directions set */
export function fits(h: SpotHour, sector: Sector | null): boolean {
  return sector === null || inSector(sector, h.dir);
}

/** A CSS gradient over `from`…`to`, each hour reaching halfway to its neighbours. */
export function glanceBand(hours: readonly SpotHour[], from: number, to: number, sector: Sector | null): string {
  const span = Math.max(1, to - from);
  const at = (t: number) => `${(Math.min(1, Math.max(0, (t - from) / span)) * 100).toFixed(2)}%`;
  const inRange = hours.filter((h) => h.validAt >= from && h.validAt <= to);
  if (inRange.length === 0) return 'transparent';
  const stops = inRange.flatMap((h, i) => {
    const bg = windBand(h.wind).bg;
    const c = fits(h, sector) ? bg : `color-mix(in srgb, ${bg} ${FADED}%, transparent)`;
    const prev = inRange[i - 1]?.validAt ?? h.validAt;
    const next = inRange[i + 1]?.validAt ?? h.validAt;
    return [`${c} ${at((prev + h.validAt) / 2)}`, `${c} ${at((h.validAt + next) / 2)}`];
  });
  return `linear-gradient(to right, ${stops.join(', ')})`;
}

/** The hour with the strongest wind from the spot's directions after `now`; null when none. */
export function peak(hours: readonly SpotHour[], sector: Sector | null, now: number): SpotHour | null {
  return hours
    .filter((h) => h.validAt >= now - 1_800 && fits(h, sector))
    .reduce<SpotHour | null>((best, h) => (best === null || h.wind > best.wind ? h : best), null);
}
