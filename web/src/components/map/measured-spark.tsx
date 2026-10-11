import { useQuery } from '@tanstack/react-query';
import type { WindUnit } from '@/bindings/WindUnit';
import { historyQuery } from '@/lib/api';
import { spark } from '@/lib/spark';
import { formatWind, UNIT_LABEL } from '@/lib/units';
import { useNow } from '@/lib/use-now';
import { m } from '@/paraglide/messages';

const HOURS = 6;
const W = 240;
const H = 34;

/** A station's last six hours measured: wind as a blue line, gusts as red dots, in the history
 * chart's colours. */
export function MeasuredSpark({ slug, unit }: { slug: string; unit: WindUnit }) {
  // In steps of ten minutes, as measurements come, so the query does not change every second
  const to = Math.ceil(useNow() / 600) * 600;
  const from = to - HOURS * 3600;
  const history = useQuery(historyQuery(slug, from, to, true));
  const s = history.data && spark(history.data.observations, from, to, W, H);
  if (!s) return null;
  const winds = history.data?.observations.flatMap((o) => (o.wind !== null ? [o.wind] : [])) ?? [];
  const label = m.map_measuredCurve({
    hours: HOURS,
    min: formatWind(Math.min(...winds), unit),
    max: formatWind(Math.max(...winds), unit),
    unit: UNIT_LABEL[unit],
  });
  return (
    <figure className="m-0 flex flex-col gap-0.5">
      <svg viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" role="img" aria-label={label} className="h-9 w-full">
        <path d={s.area} className="fill-chart-wind/15" />
        <path
          d={s.line}
          className="fill-none stroke-chart-wind"
          strokeWidth={1.5}
          vectorEffect="non-scaling-stroke"
          strokeLinejoin="round"
        />
        {s.gusts.map((g) => (
          <circle key={`${g.x},${g.y}`} cx={g.x} cy={g.y} r={1.8} className="fill-chart-gust" />
        ))}
      </svg>
      <figcaption className="flex justify-between font-mono text-[10px] text-muted-foreground" aria-hidden="true">
        <span>−{HOURS} h</span>
        <span>
          max {formatWind(s.max, unit)} {UNIT_LABEL[unit]}
        </span>
      </figcaption>
    </figure>
  );
}
