/**
 * Do the models agree? The spread (highest minus lowest mean wind) of the models at one time:
 * up to 4 kn high, up to 8 kn medium, above that low.
 */
import type { Forecast } from '@/bindings/Forecast';
import { localClock, MIX } from './forecast';

export type Agreement = 'high' | 'medium' | 'low';

export interface Spread {
  /** Unix seconds */
  time: number;
  level: Agreement;
  /** kn */
  spread: number;
  values: { model: string; wind: number }[];
}

export function levelOf(spread: number): Agreement {
  return spread <= 4 ? 'high' : spread <= 8 ? 'medium' : 'low';
}

/** Each model's wind at `time` (the mix left out); null when fewer than two have one. */
export function spreadAt(fc: Forecast, time: number): Spread | null {
  const values = fc.series
    .filter((s) => s.model !== MIX)
    .flatMap((s) => {
      const d = time - s.start;
      const i = d % s.step === 0 ? d / s.step : -1;
      const wind = i >= 0 ? s.wind[i] : null;
      return wind == null ? [] : [{ model: s.model, wind }];
    });
  if (values.length < 2) return null;
  const winds = values.map((v) => v.wind);
  const spread = Math.round(Math.max(...winds) - Math.min(...winds));
  return { time, level: levelOf(spread), spread, values };
}

/** The best time ahead: the windiest daytime hour (7–19 local) of the next 48 hours by the
 * mix (else the first series), where at least two models have a value. */
export function bestTime(fc: Forecast, now: number): number | null {
  const ref = fc.series.find((s) => s.model === MIX) ?? fc.series[0];
  if (!ref) return null;
  const clock = localClock(fc.timezone ?? 'UTC');
  let best: { time: number; wind: number } | null = null;
  for (let i = 0; i < ref.wind.length; i++) {
    const time = ref.start + i * ref.step;
    const wind = ref.wind[i];
    if (time < now || time > now + 48 * 3600 || wind == null) continue;
    const { hour } = clock(time);
    if (hour < 7 || hour > 19 || !spreadAt(fc, time)) continue;
    if (!best || wind > best.wind) best = { time, wind };
  }
  return best?.time ?? null;
}

/** A day's agreement: the spread at its windiest hour among `hours` (local) by the mix. */
export function dayAgreement(fc: Forecast, times: number[]): Spread | null {
  const ref = fc.series.find((s) => s.model === MIX);
  let pick: { time: number; wind: number } | null = null;
  for (const time of times) {
    const i = ref ? (time - ref.start) / ref.step : -1;
    const wind = ref && Number.isInteger(i) && i >= 0 ? (ref.wind[i] ?? null) : null;
    if (wind != null && (!pick || wind > pick.wind) && spreadAt(fc, time)) pick = { time, wind };
  }
  return pick ? spreadAt(fc, pick.time) : null;
}

/** The top of the agreement strip in kn: 30, or the next ten above the strongest model, so
 * every model's dot and the spread stay on it. */
export function stripTop(winds: readonly number[]): number {
  return Math.max(30, Math.ceil(Math.max(0, ...winds) / 10) * 10);
}
