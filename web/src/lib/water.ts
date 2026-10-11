/** Water level trend from a gauge's history. */
import type { LevelPoint } from '@/bindings/LevelPoint';

export type Trend = 'rising' | 'falling' | 'steady';

/** A change of less than this counts as steady; weirs move the level a few cm on their own. */
const STEADY_CM = 3;

/** The point closest to `at`, if one lies within `within` seconds. */
function at(history: readonly LevelPoint[], time: number, within: number): LevelPoint | null {
  let best: LevelPoint | null = null;
  for (const p of history) {
    if (Math.abs(p.at - time) <= within && (!best || Math.abs(p.at - time) < Math.abs(best.at - time))) best = p;
  }
  return best;
}

/** Rising, falling or steady against the level a day earlier (shown beside it); without that
 * value, against three hours earlier. */
export function levelTrend(history: readonly LevelPoint[]): { trend: Trend | null; yesterday: number | null } {
  const last = history.at(-1);
  if (!last) return { trend: null, yesterday: null };
  const day = at(history, last.at - 86_400, 3600);
  const before = day ?? at(history, last.at - 3 * 3600, 3600);
  const change = before ? last.cm - before.cm : null;
  const trend: Trend | null =
    change === null ? null : change >= STEADY_CM ? 'rising' : change <= -STEADY_CM ? 'falling' : 'steady';
  return { trend, yesterday: day ? day.cm : null };
}
