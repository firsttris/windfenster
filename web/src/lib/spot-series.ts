/** A place's hours as the map draws them: the finer model while it has hours there, the base
 * model around them; and the colour band the timeline shows for them. */
import type { MapSeries } from '@/bindings/MapSeries';
import type { SeriesPoint } from '@/bindings/SeriesPoint';
import { windBand } from '@/ui/wind-scale';

export interface SpotHour extends SeriesPoint {
  model: string;
}

export function spotHours(series: MapSeries, base: string | null, detail: string | null): SpotHour[] {
  const of = (model: string | null): SpotHour[] =>
    (series.layers.find((l) => l.model === model)?.points ?? []).map((p) => ({ ...p, model: model as string }));
  const fine = of(detail);
  const first = fine[0]?.validAt ?? Number.POSITIVE_INFINITY;
  const last = fine.at(-1)?.validAt ?? Number.NEGATIVE_INFINITY;
  return [...fine, ...of(base).filter((p) => p.validAt < first || p.validAt > last)].sort(
    (a, b) => a.validAt - b.validAt,
  );
}

/** A CSS gradient over `from`…`to` in the wind scale's colours of the hours, each hour's colour
 * reaching halfway to its neighbours; transparent where there is none. */
export function windGradient(hours: readonly SpotHour[], from: number, to: number, gusts = false): string {
  const span = Math.max(1, to - from);
  const at = (t: number) => `${(Math.min(1, Math.max(0, (t - from) / span)) * 100).toFixed(2)}%`;
  const inRange = hours.filter((h) => h.validAt >= from && h.validAt <= to);
  if (inRange.length === 0) return 'transparent';
  const stops = inRange.flatMap((h, i) => {
    const kn = gusts ? (h.gust ?? h.wind) : h.wind;
    const c = windBand(kn).bg;
    const prev = inRange[i - 1]?.validAt ?? h.validAt;
    const next = inRange[i + 1]?.validAt ?? h.validAt;
    return [`${c} ${at((prev + h.validAt) / 2)}`, `${c} ${at((h.validAt + next) / 2)}`];
  });
  return `linear-gradient(to right, ${stops.join(', ')})`;
}
