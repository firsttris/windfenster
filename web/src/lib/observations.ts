/** Measured values next to the forecast. */
import type { Observation } from '@/bindings/Observation';

/** Measurements shown as a table row under the forecast. */
export interface Measured {
  /** Where they come from, e.g. "METAR EDSB" */
  source: string;
  observations: Observation[];
}

/** A measurement counts for a forecast hour this close to it (METARs come at :20 and :50). */
export const MATCH_S = 20 * 60;

/** Older than this, the newest measurement is marked as out of date. */
export const STALE_S = 2 * 3600;

/** The measurement closest to `time` within `within` seconds; `obs` oldest first. */
export function nearest(obs: readonly Observation[], time: number, within = MATCH_S): Observation | null {
  let best: Observation | null = null;
  for (const o of obs) {
    const d = Math.abs(o.at - time);
    if (d <= within && (best === null || d < Math.abs(best.at - time))) best = o;
    if (o.at > time + within) break;
  }
  return best;
}

/** The newest measurement with a wind value. */
export function latest(obs: readonly Observation[]): Observation | null {
  for (let i = obs.length - 1; i >= 0; i--) if (obs[i].wind !== null) return obs[i];
  return null;
}
