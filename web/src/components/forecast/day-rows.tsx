/** One day on a phone, as in the mockup after Windfinder: the hours under one another, each a row
 * with its wind as a bar behind it (as long as the wind, in the scale's colour) and a line along
 * its foot that runs on to the gust. On the row: time, direction, wind and gust, the TAF where it
 * starts (a dashed line while it holds), the gear that fits, the weather. Hours that are over
 * are slightly shaded, and a line marks now. */
import type { GearItem } from '@/bindings/GearItem';
import type { Taf } from '@/bindings/Taf';
import type { WindUnit } from '@/bindings/WindUnit';
import { nowIndex, overHours } from '@/lib/day-now';
import type { Cell, Day } from '@/lib/forecast';
import { compass, formatNumber, hour } from '@/lib/format';
import { gearFor } from '@/lib/gear';
import { tafSegments } from '@/lib/taf-segments';
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
  taf?: Taf | null;
  gear?: readonly GearItem[];
  className?: string;
}

const COLS = 'grid-cols-[30px_28px_70px_1fr_74px]';

/** A bar's share of the row: linear up to 30 kn, full beyond */
const share = (kn: number) => Math.min(1, Math.max(0, kn) / 30);

export function DayRows({ day, unit, taf, gear = [], className }: Props) {
  const cells = day.cells.filter((c) => c.wind !== null || c.temp !== null);
  const shown = cells;
  // The TAF in sections over the rows shown: its value on the row where it starts, a dashed line
  // below as long as it holds (lib/taf-segments.ts)
  const tafAt = new Map<number, { text: string; high: number; start: boolean }>();
  for (const seg of tafSegments(shown, taf, unit)) {
    if (seg.text === null) continue;
    for (let i = seg.from; i < seg.from + seg.span; i++)
      tafAt.set(shown[i].time, { text: seg.text, high: seg.high, start: i === seg.from });
  }
  // Today: the first hour still to come gets the "now" line, the hours before it are shaded
  const over = overHours(shown);
  const nowAt = nowIndex(shown);
  return (
    <div className={className}>
      <div className={cn('grid gap-2 px-3 pb-1.5 font-mono text-[10px] text-dim uppercase tracking-[0.06em]', COLS)}>
        <span>{m.rows_time()}</span>
        <span />
        <span>
          {m.table_wind()} {UNIT_LABEL[unit]}
        </span>
        <span className="truncate">{m.rows_checks()}</span>
        <span className="text-right">{m.table_weather()}</span>
      </div>
      {shown.map((c, i) => (
        <Row key={c.time} cell={c} unit={unit} gear={gear} taf={tafAt.get(c.time)} now={i === nowAt} over={over[i]} />
      ))}
    </div>
  );
}

function Row({
  cell: c,
  unit,
  taf,
  gear,
  now,
  over,
}: {
  cell: Cell;
  unit: WindUnit;
  taf?: { text: string; high: number; start: boolean };
  gear: readonly GearItem[];
  /** The first hour still to come, under the past ones */
  now: boolean;
  over: boolean;
}) {
  const wind = c.wind ?? 0;
  const gust = Math.max(c.gust ?? 0, wind);
  const wb = windBand(wind).bg;
  const gb = windBand(gust).bg;
  const fit = gear.length > 0 && c.wind !== null ? gearFor(gear, c.wind, c.gust) : null;
  return (
    <div
      className={cn(
        'relative isolate grid min-h-[58px] items-center gap-2 border-line border-t py-1.5 pr-3 pl-2.5',
        COLS,
        over && 'bg-foreground/[0.04]',
        now && 'border-t-2 border-t-foreground/60',
      )}
    >
      {now && (
        <span className="absolute top-0 right-2 z-10 -translate-y-1/2 rounded bg-foreground px-1.5 font-bold font-mono text-[10px] text-background">
          {m.table_now()}
        </span>
      )}
      {c.wind !== null && (
        <>
          {/* The wind as the row's background, fading towards its end; paler on the light theme so
              the dark text on it stays readable */}
          <span
            aria-hidden="true"
            className="absolute inset-y-0 left-0 -z-10 opacity-45 dark:opacity-70"
            style={{
              width: `${Math.max(6, share(wind) * 100)}%`,
              background: `linear-gradient(90deg, ${wb}, ${wb}99 75%, ${wb}38)`,
            }}
          />
          {/* Along the foot: the wind, then on to the gust */}
          <span
            aria-hidden="true"
            className="absolute bottom-0 left-0 h-1.5"
            style={{ width: `${share(wind) * 100}%`, background: wb }}
          />
          <span
            aria-hidden="true"
            className="absolute bottom-0 h-1.5"
            style={{
              left: `${share(wind) * 100}%`,
              width: `${Math.max(0, share(gust) - share(wind)) * 100}%`,
              background: gb,
            }}
          />
        </>
      )}
      <span className="font-mono text-[12px]">{hour(c.hour)}</span>
      {c.dir === null ? (
        <Dash />
      ) : (
        <span className="flex flex-col items-center gap-px">
          <WindArrow deg={c.dir} className="size-4" />
          <span className="font-mono text-[9px]">{compass(c.dir)}</span>
        </span>
      )}
      <span className="flex flex-col leading-tight">
        {c.wind === null ? (
          <Dash />
        ) : (
          <span className="font-bold font-mono text-[19px]">{formatWind(c.wind, unit)}</span>
        )}
        {c.gust !== null && (
          <span className="whitespace-nowrap text-[11px]">
            {m.table_gusts()} <b className="font-mono">{formatWind(gust, unit)}</b>
          </span>
        )}
      </span>
      <span className="flex min-w-0 flex-col items-start gap-1">
        {taf?.start && (
          <span
            className="inline-flex rounded-md border border-dashed bg-background/80 px-1.5 font-mono font-semibold text-[11px]"
            style={{ borderColor: windBand(taf.high).bg }}
          >
            {m.taf_row()} {taf.text}
          </span>
        )}
        {taf && !taf.start && (
          <span
            aria-hidden="true"
            className="ml-2.5 block h-4 border-l border-dashed"
            style={{ borderColor: windBand(taf.high).bg }}
          />
        )}
        {fit !== null && fit !== 'none' && <GearFitCell fit={fit} className="items-start" />}
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
    </div>
  );
}
