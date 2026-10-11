/** The data of the model comparison chart: today's daytime hours, every model and the
 * measurements so far, in the chosen unit. */
import type { Forecast } from '@/bindings/Forecast';
import type { Observation } from '@/bindings/Observation';
import type { WindUnit } from '@/bindings/WindUnit';
import { localClock } from './forecast';
import { KMH_PER_KN, MS_PER_KN } from './units';

export const FIRST_HOUR = 7;
export const LAST_HOUR = 19;

const FACTOR: Record<WindUnit, number> = { kn: 1, ms: MS_PER_KN, kmh: KMH_PER_KN };

export interface CompareData {
  /** Unix seconds: the full hours and the measurement times, sorted */
  x: number[];
  /** Per model (as in `models`), its value at each x; null between its hours */
  lines: (number | null)[][];
  models: string[];
  /** The measurement at each x, null elsewhere */
  measured: (number | null)[];
  from: number;
  to: number;
}

/** Today (station time) from 7 to 19 o'clock. */
export function compareData(
  fc: Forecast,
  observations: readonly Observation[],
  now: number,
  unit: WindUnit,
): CompareData | null {
  const clock = localClock(fc.timezone ?? 'UTC');
  const today = clock(now).date;
  const hours: number[] = [];
  // The hours of today between 7 and 19, found by walking the full hours around now.
  for (let t = now - (now % 3600) - 24 * 3600; t <= now + 24 * 3600; t += 3600) {
    const c = clock(t);
    if (c.date === today && c.hour >= FIRST_HOUR && c.hour <= LAST_HOUR) hours.push(t);
  }
  if (hours.length === 0) return null;
  const from = hours[0];
  const to = hours[hours.length - 1];
  const f = FACTOR[unit];
  const obs = observations.filter((o) => o.at >= from && o.at <= to && o.wind !== null);
  const x = [...new Set([...hours, ...obs.map((o) => o.at)])].sort((a, b) => a - b);
  const models = fc.series.map((s) => s.model);
  const lines = fc.series.map((s) =>
    x.map((t) => {
      const d = t - s.start;
      const i = d % s.step === 0 ? d / s.step : -1;
      const v = i >= 0 ? s.wind[i] : null;
      return v == null ? null : Math.round(v * f * 10) / 10;
    }),
  );
  const byTime = new Map(obs.map((o) => [o.at, o.wind as number]));
  const measured = x.map((t) => {
    const v = byTime.get(t);
    return v === undefined ? null : Math.round(v * f * 10) / 10;
  });
  return { x, lines, models, measured, from, to };
}
