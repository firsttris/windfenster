/**
 * The forecast table: ten days in three stages, as in the plan. Days 1–2 hourly from the
 * chosen model (a regional one for Superforecast), days 3–7 every 3 hours, days 8–10 every
 * 6 hours as a paler trend. Where the chosen model ends, the best global model takes over.
 * Times are the place's local time.
 */
import type { Forecast } from '@/bindings/Forecast';
import type { ModelSeries } from '@/bindings/ModelSeries';

export type Mode = 'super' | 'global';
export type Stage = 'hourly' | 'threeHourly' | 'trend';

export const DAYS = 10;

/** Local hours shown per stage: daytime, when people are on the water. */
export const STAGE_HOURS: Record<Stage, readonly number[]> = {
  hourly: [7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19],
  threeHourly: [6, 9, 12, 15, 18, 21],
  trend: [6, 12, 18],
};

/** Hours a cell stands for (rain is summed over them). */
export const STAGE_STEP: Record<Stage, number> = { hourly: 1, threeHourly: 3, trend: 6 };

export function stageOf(dayIndex: number): Stage {
  return dayIndex <= 1 ? 'hourly' : dayIndex <= 6 ? 'threeHourly' : 'trend';
}

export interface Cell {
  /** Unix seconds */
  time: number;
  hour: number;
  wind: number | null;
  gust: number | null;
  dir: number | null;
  temp: number | null;
  /** mm over the cell's step */
  rain: number | null;
  cloud: number | null;
  past: boolean;
}

export interface Day {
  /** Local date, YYYY-MM-DD */
  date: string;
  stage: Stage;
  /** Model the day's values come from */
  source: string;
  cells: Cell[];
}

/** The first model of a class (in the default order) that has values here. */
export function defaultModel(fc: Forecast, mode: Mode): string | null {
  const wanted = fc.models.filter((m) => m.class === mode).map((m) => m.id);
  return fc.series.find((s) => wanted.includes(s.model))?.model ?? null;
}

export function modeOf(fc: Forecast, modelId: string): Mode {
  return fc.models.find((m) => m.id === modelId)?.class === 'super' ? 'super' : 'global';
}

/** Local date and hour of a Unix time in a time zone. */
export function localClock(timeZone: string): (unix: number) => { date: string; hour: number } {
  const fmt = new Intl.DateTimeFormat('en-CA', {
    timeZone,
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    hourCycle: 'h23',
  });
  return (unix) => {
    const parts = Object.fromEntries(fmt.formatToParts(new Date(unix * 1000)).map((p) => [p.type, p.value]));
    return { date: `${parts.year}-${parts.month}-${parts.day}`, hour: Number(parts.hour) };
  };
}

interface Indexed {
  series: ModelSeries;
  at: Map<string, number>;
}

function indexSeries(series: ModelSeries, clock: ReturnType<typeof localClock>): Indexed {
  const at = new Map<string, number>();
  for (let i = 0; i < series.wind.length; i++) {
    const { date, hour } = clock(series.start + i * series.step);
    const key = `${date}|${hour}`;
    // On the autumn clock change an hour repeats; keep the first.
    if (!at.has(key)) at.set(key, i);
  }
  return { series, at };
}

const value = (arr: (number | null)[], i: number | undefined) => (i === undefined ? null : (arr[i] ?? null));

function rainOver(s: ModelSeries, i: number | undefined, hours: number): number | null {
  if (i === undefined) return null;
  const perStep = Math.max(1, Math.round((hours * 3600) / s.step));
  let sum = 0;
  for (let k = i - perStep + 1; k <= i; k++) {
    const v = s.rain[k];
    if (v === null || v === undefined) return k === i ? null : (s.rain[i] ?? null);
    sum += v;
  }
  return Math.round(sum * 10) / 10;
}

/** Builds the table for the chosen model at `now` (Unix seconds). */
export function buildDays(fc: Forecast, selected: string, now: number): Day[] {
  const clock = localClock(fc.timezone ?? 'UTC');
  const bySource = new Map(fc.series.map((s) => [s.model, s]));
  const chosen = bySource.get(selected);
  const globalId = defaultModel(fc, 'global');
  const fallback = globalId ? bySource.get(globalId) : undefined;
  const base = fallback ?? chosen;
  if (!base) return [];
  const indexed = new Map<string, Indexed>();
  for (const s of [chosen, fallback]) if (s) indexed.set(s.model, indexSeries(s, clock));

  const today = clock(now).date;
  const dates: string[] = [];
  for (const s of indexed.values()) {
    for (const key of s.at.keys()) {
      const date = key.slice(0, 10);
      if (date >= today && !dates.includes(date)) dates.push(date);
    }
  }
  dates.sort();

  return dates.slice(0, DAYS).map((date, d) => {
    const stage = stageOf(d);
    const hours = STAGE_HOURS[stage];
    const covers = (s: Indexed | undefined): s is Indexed =>
      !!s && hours.every((h) => value(s.series.wind, s.at.get(`${date}|${h}`)) !== null);
    const first = chosen ? indexed.get(chosen.model) : undefined;
    const second = fallback ? indexed.get(fallback.model) : undefined;
    // base exists, so at least one of them does
    const src = [first, second].find(covers) ?? first ?? (second as Indexed);
    const cells = hours.map((hour): Cell => {
      const i = src.at.get(`${date}|${hour}`);
      const s = src.series;
      const time = i === undefined ? 0 : s.start + i * s.step;
      return {
        time,
        hour,
        wind: value(s.wind, i),
        gust: value(s.gust, i),
        dir: value(s.dir, i),
        temp: value(s.temp, i),
        rain: rainOver(s, i, STAGE_STEP[stage]),
        cloud: value(s.cloud, i),
        past: i !== undefined && time + STAGE_STEP[stage] * 3600 <= now,
      };
    });
    return { date, stage, source: src.series.model, cells };
  });
}

export type Weather = 'sun' | 'partly' | 'cloud' | 'rain';

/** A symbol from cloud cover (%) and rain (mm over the cell). */
export function weatherOf(cloud: number | null, rain: number | null): Weather | null {
  if (rain !== null && rain >= 0.3) return 'rain';
  if (cloud === null) return null;
  return cloud < 20 ? 'sun' : cloud < 65 ? 'partly' : 'cloud';
}

/** Index 0–15 of the compass point a direction (degrees) is closest to: 0 = N, 4 = E … */
export function compassIndex(deg: number): number {
  return Math.round((((deg % 360) + 360) % 360) / 22.5) % 16;
}
