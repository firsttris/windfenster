/**
 * Measurements over a day, as Windfinder shows them: "today" is the last 24 hours, the days
 * before run from midnight to midnight (station time), ten of them.
 */
import type { Observation } from '@/bindings/Observation';
import { localClock } from './forecast';

export const DAYS_BACK = 10;

/** Beaufort scale: the lowest speed of each force from 1 to 12, kn. */
export const BEAUFORT_KN = [1, 4, 7, 11, 17, 22, 28, 34, 41, 48, 56, 64] as const;

export interface DayOption {
  /** "today" or the local date YYYY-MM-DD */
  key: string;
  /** Unix seconds, [from, to) */
  from: number;
  to: number;
  /** Local date of the day (today: of `to`) */
  date: string;
}

/** Seconds the time zone is ahead of UTC at `unix`. */
function offset(unix: number, timeZone: string): number {
  const p = Object.fromEntries(
    new Intl.DateTimeFormat('en-CA', {
      timeZone,
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
      hourCycle: 'h23',
    })
      .formatToParts(new Date(unix * 1000))
      .map((x) => [x.type, x.value]),
  );
  const wall = Date.UTC(+p.year, +p.month - 1, +p.day, +p.hour, +p.minute, +p.second) / 1000;
  return wall - unix;
}

/** Unix seconds of local midnight at the start of `date` (YYYY-MM-DD). */
export function localMidnight(date: string, timeZone: string): number {
  const [y, m, d] = date.split('-').map(Number);
  const utc = Date.UTC(y, m - 1, d) / 1000;
  // Twice: the offset at midnight can differ from the one at the first guess (DST).
  const guess = utc - offset(utc, timeZone);
  return utc - offset(guess, timeZone);
}

/** Today (the last 24 hours up to `now`) and the ten days before, newest first. */
export function dayOptions(now: number, timeZone: string): DayOption[] {
  const clock = localClock(timeZone);
  const today = clock(now).date;
  const out: DayOption[] = [{ key: 'today', from: now - 24 * 3600, to: now, date: today }];
  let end = localMidnight(today, timeZone);
  for (let i = 0; i < DAYS_BACK; i++) {
    // Noon of the day before, so a 23- or 25-hour day does not skip one.
    const date = clock(end - 12 * 3600).date;
    const from = localMidnight(date, timeZone);
    out.push({ key: date, from, to: end, date });
    end = from;
  }
  return out;
}

export interface DayStats {
  /** kn */
  maxWind: number | null;
  maxGust: number | null;
  meanWind: number | null;
  /** Where the wind came from most, as the mean wind vector; null when calm */
  mainDir: number | null;
}

export function dayStats(obs: readonly Observation[]): DayStats {
  const winds = obs.flatMap((o) => (o.wind === null ? [] : [o.wind]));
  const gusts = obs.flatMap((o) => (o.gust === null ? [] : [o.gust]));
  let u = 0;
  let v = 0;
  for (const o of obs) {
    if (o.wind !== null && o.dir !== null) {
      const r = (o.dir * Math.PI) / 180;
      u += o.wind * Math.sin(r);
      v += o.wind * Math.cos(r);
    }
  }
  const mainDir = Math.hypot(u, v) > 1e-6 ? ((((Math.atan2(u, v) * 180) / Math.PI) % 360) + 360) % 360 : null;
  return {
    maxWind: winds.length ? Math.max(...winds) : null,
    maxGust: gusts.length ? Math.max(...gusts) : null,
    meanWind: winds.length ? winds.reduce((a, b) => a + b, 0) / winds.length : null,
    mainDir,
  };
}

/** Measurements spread so that arrows on a chart `width` px wide keep `minPx` apart. */
export function arrowPicks(
  obs: readonly Observation[],
  from: number,
  to: number,
  width: number,
  minPx = 28,
): Observation[] {
  const step = Math.max(3600, ((to - from) * minPx) / Math.max(width, 1));
  const out: Observation[] = [];
  for (let t = from + step / 2; t < to; t += step) {
    let best: Observation | null = null;
    for (const o of obs) {
      if (o.dir === null || o.wind === null || Math.abs(o.at - t) > step / 2) continue;
      if (!best || Math.abs(o.at - t) < Math.abs(best.at - t)) best = o;
    }
    if (best) out.push(best);
  }
  return out;
}

/** Size of the card that shows the values under the pointer, in CSS pixels */
export const TIP = { width: 156, height: 92, gap: 14 } as const;

/** Where that card goes for a point at (x, y) of a chart `width` wide: centred above the
 * point, kept inside the chart at its edges, and below the point when there is no room above. */
export function tipPlacement(x: number, y: number, width: number): { left: number; top: number } {
  const left = Math.min(Math.max(0, x - TIP.width / 2), Math.max(0, width - TIP.width));
  const above = y - TIP.gap - TIP.height;
  return { left, top: above >= 0 ? above : y + TIP.gap };
}

/** Indices of values that stand alone between gaps: a line cannot show them, a dot can.
 * METARs name gusts only when there are some, so a gust often stands alone. */
export function isolated(values: ArrayLike<number | null | undefined>): number[] {
  const out: number[] = [];
  for (let i = 0; i < values.length; i++) {
    if (values[i] != null && values[i - 1] == null && values[i + 1] == null) out.push(i);
  }
  return out;
}
