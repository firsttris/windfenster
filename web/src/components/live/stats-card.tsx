import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import type { MonthStats } from '@/bindings/MonthStats';
import type { Station } from '@/bindings/Station';
import { statsQuery } from '@/lib/api';
import { compass, formatNumber } from '@/lib/format';
import { intlLocale } from '@/lib/i18n';
import { useUnit } from '@/lib/prefs';
import { formatWind, UNIT_LABEL } from '@/lib/units';
import { cn } from '@/lib/utils';
import { petal, rings, rose, STATS_BANDS_KN, sectorDegrees, wholeYear, windyShare } from '@/lib/wind-stats';
import { m } from '@/paraglide/messages';
import { Card } from '@/ui/card';
import { WindArrow } from '@/ui/wind-arrow';
import { windBand } from '@/ui/wind-scale';

const R = 120;
/** The calm circle in the middle */
const HOLE = 16;
const LABEL_AT = (78.75 * Math.PI) / 180;
const percent = (share: number) => `${formatNumber(share * 100)} %`;

/** A point at `share` of the way out (0 the hole, 1 the edge) in a direction (degrees, north 0) */
function at(share: number, deg: number): [number, number] {
  const r = HOLE + share * (R - HOLE);
  const a = (deg * Math.PI) / 180;
  return [r * Math.sin(a), -r * Math.cos(a)];
}

/** An annulus piece from `from` to `to` (shares) over a sector, a little narrower than it */
function wedge(sector: number, from: number, to: number): string {
  const mid = sectorDegrees(sector);
  const [a, b] = [mid - 9.5, mid + 9.5];
  const [p1, p2, p3, p4] = [at(from, a), at(to, a), at(to, b), at(from, b)];
  const ro = HOLE + to * (R - HOLE);
  const ri = HOLE + from * (R - HOLE);
  return `M${p1} L${p2} A${ro},${ro} 0 0 1 ${p3} L${p4} A${ri},${ri} 0 0 0 ${p1} Z`;
}

function RoseChart({ month }: { month: MonthStats }) {
  const r = rose(month);
  const top = rings(r.max);
  const scale = top[top.length - 1];
  const points = m.compass_points().split(',');
  return (
    <svg
      viewBox={`${-R - 22} ${-R - 22} ${2 * R + 44} ${2 * R + 44}`}
      role="img"
      aria-label={m.stats_roseLabel({
        main: r.main !== null ? compass(sectorDegrees(r.main)) : '–',
        calm: percent(r.calm),
      })}
      className="mx-auto block w-full max-w-[320px]"
    >
      {top.map((ring, i) => {
        const radius = HOLE + (ring / scale) * (R - HOLE);
        // With many rings every other one is named, counted from the outermost
        const named = top.length <= 3 || (top.length - 1 - i) % 2 === 0;
        return (
          <g key={ring}>
            <circle r={radius} fill="none" className="stroke-line" strokeDasharray="2 3" />
            {/* Between east-north-east and east, where petals leave a gap */}
            {named && (
              <text
                x={radius * Math.sin(LABEL_AT)}
                y={-radius * Math.cos(LABEL_AT) - 2}
                className="fill-muted-foreground text-[9px]"
              >
                {percent(ring)}
              </text>
            )}
          </g>
        );
      })}
      {[0, 4, 8, 12].map((s) => {
        const [x, y] = at(1.13, sectorDegrees(s));
        return (
          <text key={s} x={x} y={y + 4} textAnchor="middle" className="fill-muted-foreground font-medium text-[12px]">
            {points[s]}
          </text>
        );
      })}
      {r.sectors.map((bands, sector) =>
        petal(bands, scale).map((p) => (
          <path
            // biome-ignore lint/suspicious/noArrayIndexKey: sectors and bands are fixed positions
            key={`${sector}-${p.band}`}
            d={wedge(sector, p.from, p.to)}
            fill={windBand(STATS_BANDS_KN[p.band]).bg}
            className="stroke-card"
            strokeWidth={0.75}
          />
        )),
      )}
      <circle r={HOLE - 2} className="fill-card stroke-line" />
      <text y={3.5} textAnchor="middle" className="fill-muted-foreground text-[9px]">
        {percent(r.calm)}
      </text>
    </svg>
  );
}

function Legend() {
  const [unit] = useUnit();
  return (
    <ul className="m-0 flex list-none flex-wrap gap-x-3 gap-y-1 p-0 text-muted-foreground text-xs">
      {STATS_BANDS_KN.map((from, i) => {
        const next = STATS_BANDS_KN[i + 1];
        const label =
          next !== undefined ? `${formatWind(from, unit)}–${formatWind(next, unit)}` : `${formatWind(from, unit)}+`;
        return (
          <li key={from} className="flex items-center gap-1.5">
            <span className="size-2.5 rounded-sm" style={{ background: windBand(from).bg }} aria-hidden="true" />
            {label}
          </li>
        );
      })}
      <li>{UNIT_LABEL[unit]}</li>
    </ul>
  );
}

/** A station's wind over ten whole years: the windy days per month, and the rose of the month
 * picked or of the whole year. */
export function StatsCard({ station }: { station: Station }) {
  const q = useQuery(statsQuery(station.slug));
  const [unit] = useUnit();
  // The current month first: what to expect now
  const [picked, setPicked] = useState<number | 'year'>(() => new Date().getMonth());
  if (q.isError || (q.data && !q.data.pending && !q.data.stats)) return null;
  const stats = q.data?.stats;
  const months = new Intl.DateTimeFormat(intlLocale(), { month: 'short', timeZone: 'UTC' });
  const monthName = (i: number) => months.format(Date.UTC(2024, i, 15));
  const narrowName = (i: number) =>
    new Intl.DateTimeFormat(intlLocale(), { month: 'narrow', timeZone: 'UTC' }).format(Date.UTC(2024, i, 15));
  const longMonth = (i: number) =>
    new Intl.DateTimeFormat(intlLocale(), { month: 'long', timeZone: 'UTC' }).format(Date.UTC(2024, i, 15));
  const shown = stats ? (picked === 'year' ? wholeYear(stats) : stats.months[picked]) : null;
  const r = shown ? rose(shown) : null;
  const windy = shown ? windyShare(shown) : null;
  const source = station.source === 'metar' ? m.stats_sourceIem() : m.stats_sourceDwd();

  return (
    <Card aria-labelledby="stats-title">
      <div>
        <h2 id="stats-title" className="m-0 font-semibold text-lg tracking-[-0.02em]">
          {m.stats_title()}
        </h2>
        <p className="m-0 mt-1 text-[13px] text-muted-foreground">
          {stats
            ? m.stats_sub({ from: String(stats.firstYear), to: String(stats.lastYear), source })
            : m.stats_pending()}
        </p>
      </div>
      {!stats || !shown || !r ? (
        <div role="status" aria-busy="true" className="h-[300px] animate-pulse rounded-lg bg-muted" />
      ) : (
        <>
          <fieldset
            aria-label={m.stats_months()}
            className="m-0 grid grid-cols-[repeat(12,minmax(0,1fr))_auto] items-end gap-1 border-0 p-0"
          >
            {stats.months.map((month, i) => {
              const share = windyShare(month) ?? 0;
              const on = picked === i;
              return (
                <button
                  // biome-ignore lint/suspicious/noArrayIndexKey: twelve months, in order
                  key={i}
                  type="button"
                  aria-pressed={on}
                  aria-label={m.stats_monthWindy({ month: longMonth(i), share: percent(share) })}
                  onClick={() => setPicked(i)}
                  className="group flex cursor-pointer flex-col items-center gap-1 rounded-md border-0 bg-transparent p-0 outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
                >
                  <span className="font-mono text-[10px] text-muted-foreground tabular-nums">
                    {formatNumber(share * 100)}
                  </span>
                  <span className="flex h-20 w-full items-end">
                    <span
                      className={cn(
                        'w-full rounded-t-[3px] transition-colors',
                        on ? 'bg-foreground' : 'bg-muted-foreground/35 group-hover:bg-muted-foreground/60',
                      )}
                      style={{ height: `${Math.max(2, share * 100)}%` }}
                    />
                  </span>
                  <span className={cn('text-[11px]', on ? 'font-semibold text-foreground' : 'text-muted-foreground')}>
                    <span className="sm:hidden">{narrowName(i)}</span>
                    <span className="hidden sm:inline">{monthName(i).replace('.', '')}</span>
                  </span>
                </button>
              );
            })}
            <button
              type="button"
              aria-pressed={picked === 'year'}
              onClick={() => setPicked('year')}
              className={cn(
                'ml-2 cursor-pointer self-end rounded-md border px-2.5 py-1 text-xs outline-none focus-visible:ring-2 focus-visible:ring-ring/60',
                picked === 'year' ? 'border-border-strong bg-secondary text-foreground' : 'text-muted-foreground',
              )}
            >
              {m.stats_year()}
            </button>
          </fieldset>
          <p className="m-0 text-muted-foreground text-xs">
            {m.stats_windyHint({ kn: formatWind(12, unit), unit: UNIT_LABEL[unit] })}
          </p>
          <div className="grid items-center gap-5 md:grid-cols-[minmax(0,320px)_1fr]">
            <RoseChart month={shown} />
            <div className="flex flex-col gap-4">
              <h3 className="m-0 font-semibold text-base">{picked === 'year' ? m.stats_year() : longMonth(picked)}</h3>
              <dl className="m-0 grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 text-sm">
                <dt className="text-muted-foreground">{m.stats_windyDays()}</dt>
                <dd className="m-0 font-semibold">{windy === null ? '–' : percent(windy)}</dd>
                <dt className="text-muted-foreground">{m.stats_main()}</dt>
                <dd className="m-0 flex items-center gap-1.5 font-semibold">
                  {r.main === null ? (
                    '–'
                  ) : (
                    <>
                      <WindArrow deg={sectorDegrees(r.main)} className="size-4" />
                      {compass(sectorDegrees(r.main))}
                    </>
                  )}
                </dd>
                <dt className="text-muted-foreground">{m.stats_calm()}</dt>
                <dd className="m-0 font-semibold">{percent(r.calm)}</dd>
              </dl>
              <Legend />
              <p className="m-0 text-muted-foreground text-xs leading-relaxed">{m.stats_roseHint()}</p>
            </div>
          </div>
        </>
      )}
    </Card>
  );
}
