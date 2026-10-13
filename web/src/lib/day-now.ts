/** Where "now" falls in a day's hours. A cell knows it is over only when the model has a value
 * for it (lib/forecast.ts); an hour without one before an hour that is over is over too. */
interface HourCell {
  /** Unix seconds; 0 when the model has no value for the hour */
  time: number;
  past: boolean;
}

/** For every hour whether it is over */
export function overHours(cells: readonly HourCell[]): boolean[] {
  const lastPast = cells.reduce((last, c, i) => (c.past ? i : last), -1);
  return cells.map((c, i) => c.past || (c.time === 0 && i < lastPast));
}

/** The first hour still to come on a day that has hours over and hours to come; -1 on a day all
 * over or all to come */
export function nowIndex(cells: readonly HourCell[]): number {
  const over = overHours(cells);
  const i = over.findIndex((o) => !o);
  return i > 0 && over.slice(0, i).every(Boolean) ? i : -1;
}
