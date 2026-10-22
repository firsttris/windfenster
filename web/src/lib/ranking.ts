/**
 * The ranking "who hits here?" on the model chips: each chip's place and mean error, and the
 * chips of a group in the order of the ranking.
 */
import type { Model } from '@/bindings/Model';
import type { Ranking } from '@/bindings/Ranking';

/** Days of data from which a score counts (wf_core::verification::MIN_DAYS) */
export const MIN_DAYS = 14;

export interface ChipRank {
  /** Place in the ranking card, the TAF counted too, so both show the same number */
  rank: number;
  /** Mean absolute error of the first day ahead, kn */
  mae: number;
  /** Verified over 14 days or more: only then does the place order the chips */
  settled: boolean;
}

/** Place and error per model id, from the ranking's list (lowest error first). */
export function chipRanks(ranking: Ranking | null | undefined): Map<string, ChipRank> {
  return new Map(
    (ranking?.scores ?? []).map((s, i) => [s.model, { rank: i + 1, mae: s.mae, settled: s.days >= MIN_DAYS }]),
  );
}

/** The models with a settled place first, by place; the rest after them in the order given. */
export function byRank(models: readonly Model[], ranks: ReadonlyMap<string, ChipRank>): Model[] {
  const place = (m: Model) => {
    const r = ranks.get(m.id);
    return r?.settled ? r.rank : Number.POSITIVE_INFINITY;
  };
  // Array.prototype.sort is stable: equal places keep the order given
  return [...models].sort((a, b) => place(a) - place(b));
}
