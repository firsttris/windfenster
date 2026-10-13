/** One day of the forecast on a phone: the hours under one another, as Windfinder lays them out,
 * so nothing scrolls sideways. A row reads time, direction, wind with its gust, a bar in the
 * scale's colour, then the weather; what checks the hour (measured, the TAF) under the bar. */
import type { GearItem } from '@/bindings/GearItem';
import type { Taf } from '@/bindings/Taf';
import type { WindUnit } from '@/bindings/WindUnit';
import { barShare, type Cell, type Day, isNight } from '@/lib/forecast';
import { compass, formatNumber, hour } from '@/lib/format';
import { gearFor } from '@/lib/gear';
import { type Measured, nearest } from '@/lib/observations';
import { tafHour } from '@/lib/taf-text';
import { formatWind, UNIT_LABEL } from '@/lib/units';
import { cn } from '@/lib/utils';
import { m } from '@/paraglide/messages';
import { WindArrow } from '@/ui/wind-arrow';
import { windBand } from '@/ui/wind-scale';
import { GearFitCell } from '../gear-fit';
import { Dash } from './compare-cells';
import { Weather } from './shared';

interface Props {
  day: Day;
  unit: WindUnit;
  /** Unix seconds */
  now: number;
  measured?: Measured;
  taf?: Taf | null;
  gear?: readonly GearItem[];
  className?: string;
}

export function DayList({ day, unit, now, measured, taf, gear = [], className }: Props) {
  const hourly = day.stage === 'hourly';
  return (
    <ol className={cn('m-0 list-none p-0', className)}>
      {day.cells.filter(hasValues).map((c) => (
        <li
          key={c.hour}
          className={cn(
            'grid grid-cols-[30px_30px_68px_1fr_58px] items-center gap-x-2 border-line border-t px-3 py-2',
            c.past && 'opacity-40',
            !c.past && !hourly && isNight(c.hour) && 'opacity-55',
          )}
        >
          <span className="flex flex-col items-start gap-0.5">
            <span className="font-mono text-[12px] text-muted-foreground">{hour(c.hour)}</span>
            {gear.length > 0 && <GearFitCell fit={c.wind === null ? null : gearFor(gear, c.wind, c.gust)} />}
          </span>
          {c.dir === null ? (
            <Dash />
          ) : (
            <span className="flex flex-col items-center gap-0.5">
              <WindArrow deg={c.dir} className="size-5" />
              <span className="font-mono text-[10px] text-muted-foreground">{compass(c.dir)}</span>
            </span>
          )}
          <span className="flex flex-col leading-tight">
            {c.wind === null ? (
              <Dash />
            ) : (
              <span className="whitespace-nowrap font-bold text-[18px]">
                {formatWind(c.wind, unit)}
                <span className="ml-0.5 font-normal text-[11px] text-muted-foreground">{UNIT_LABEL[unit]}</span>
              </span>
            )}
            {c.gust !== null && (
              <span className="whitespace-nowrap text-[11px] text-muted-foreground">
                {m.table_gusts()} {formatWind(Math.max(c.gust, c.wind ?? 0), unit)}
              </span>
            )}
          </span>
          <span className="flex min-w-0 flex-col gap-1">
            <WindTrack cell={c} />
            <Checks cell={c} now={now} measured={measured} taf={taf} unit={unit} />
          </span>
          <span className="flex items-center justify-end gap-1.5">
            <Weather cloud={c.cloud} rain={c.rain} />
            <span className="flex flex-col items-end leading-tight">
              {c.temp === null ? <Dash /> : <span className="font-mono text-[13px]">{Math.round(c.temp)}°</span>}
              {c.rain !== null && c.rain > 0 && (
                <span className="whitespace-nowrap font-mono text-[10px] text-water">
                  {formatNumber(c.rain, c.rain < 10 ? 1 : 0)} mm
                </span>
              )}
            </span>
          </span>
        </li>
      ))}
    </ol>
  );
}

/** An hour the model has nothing for (before its run starts) is left out, not a row of dashes */
function hasValues(c: Cell): boolean {
  return c.wind !== null || c.dir !== null || c.temp !== null || c.cloud !== null;
}

/** The wind as a bar across, in its colour, the gust as a paler run beyond it */
function WindTrack({ cell }: { cell: Cell }) {
  if (cell.wind === null) return <span className="block h-2 rounded-full bg-muted" />;
  const gust = Math.max(cell.gust ?? 0, cell.wind);
  return (
    <span className="relative block h-2 rounded-full bg-muted" aria-hidden="true">
      <span
        className="absolute inset-y-0 left-0 rounded-full opacity-40"
        style={{ width: `${barShare(gust) * 100}%`, background: windBand(gust).bg }}
      />
      <span
        className="absolute inset-y-0 left-0 rounded-full"
        style={{ width: `${Math.max(4, barShare(cell.wind) * 100)}%`, background: windBand(cell.wind).bg }}
      />
    </span>
  );
}

/** What was measured at a past hour and what the TAF says for it, in one quiet line */
function Checks({
  cell,
  now,
  measured,
  taf,
  unit,
}: {
  cell: Cell;
  now: number;
  measured?: Measured;
  taf?: Taf | null;
  unit: WindUnit;
}) {
  const o = measured && cell.time <= now ? nearest(measured.observations, cell.time) : null;
  const t = taf ? tafHour(taf, cell.time) : null;
  const parts: string[] = [];
  if (o?.wind != null) {
    const gust = o.gust != null && o.gust > o.wind ? `/${formatWind(o.gust, unit)}` : '';
    parts.push(`${m.table_measured()} ${formatWind(o.wind, unit)}${gust}`);
  }
  if (t?.wind != null) {
    const range = t.low !== null && t.high !== null;
    parts.push(
      `${m.taf_row()} ${range ? `${formatWind(t.low ?? 0, unit)}–${formatWind(t.high ?? 0, unit)}` : formatWind(t.wind, unit)}`,
    );
  }
  if (parts.length === 0) return null;
  return <span className="truncate font-mono text-[10px] text-muted-foreground">{parts.join(' · ')}</span>;
}
