/** One day on a phone, as in the mockup after Windfinder: the hours under one another, each a row
 * with its wind as a bar behind it (as long as the wind, in the scale's colour) and a line along
 * its foot that runs on to the gust. On the row: time, direction, wind and gust, what was
 * measured and the TAF, the gear that fits, the weather. Hours that are over fold into one line. */
import { ChevronDown } from 'lucide-react';
import { useState } from 'react';
import type { GearItem } from '@/bindings/GearItem';
import type { Taf } from '@/bindings/Taf';
import type { WindUnit } from '@/bindings/WindUnit';
import type { Cell, Day } from '@/lib/forecast';
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

const COLS = 'grid-cols-[30px_28px_70px_1fr_74px]';

/** A bar's share of the row: linear up to 30 kn, full beyond */
const share = (kn: number) => Math.min(1, Math.max(0, kn) / 30);

export function DayRows({ day, unit, now, measured, taf, gear = [], className }: Props) {
  const [showPast, setShowPast] = useState(false);
  const cells = day.cells.filter((c) => c.wind !== null || c.temp !== null);
  const past = cells.filter((c) => c.past);
  const shown = showPast ? cells : cells.filter((c) => !c.past);
  const ctx = { unit, now, measured, taf, gear };
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
      {past.length > 0 && (
        <button
          type="button"
          aria-expanded={showPast}
          onClick={() => setShowPast(!showPast)}
          className="flex min-h-11 w-full cursor-pointer items-center justify-between gap-2 border-line border-t bg-transparent px-3.5 text-left text-[12px] text-muted-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
        >
          <span className="flex items-center gap-2">
            <ChevronDown aria-hidden="true" className={cn('size-3.5 transition-transform', showPast && 'rotate-180')} />
            {showPast ? m.rows_pastHide() : m.rows_pastShow({ count: past.length })}
          </span>
          <span className="font-mono text-[11px]">{pastSummary(past, measured, unit)}</span>
        </button>
      )}
      {shown.map((c) => (
        <Row key={c.time} cell={c} {...ctx} />
      ))}
    </div>
  );
}

function Row({
  cell: c,
  unit,
  now,
  measured,
  taf,
  gear,
}: {
  cell: Cell;
  unit: WindUnit;
  now: number;
  measured?: Measured;
  taf?: Taf | null;
  gear: readonly GearItem[];
}) {
  const wind = c.wind ?? 0;
  const gust = Math.max(c.gust ?? 0, wind);
  const wb = windBand(wind).bg;
  const gb = windBand(gust).bg;
  const o = measured && c.time <= now ? nearest(measured.observations, c.time) : null;
  const t = taf ? tafHour(taf, c.time) : null;
  const fit = gear.length > 0 && c.wind !== null ? gearFor(gear, c.wind, c.gust) : null;
  return (
    <div
      className={cn(
        'relative isolate grid min-h-[58px] items-center gap-2 border-line border-t py-1.5 pr-3 pl-2.5',
        COLS,
        c.past && 'opacity-45',
      )}
    >
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
        {o?.wind != null && (
          <span
            className="inline-flex items-baseline gap-px rounded-md border-2 bg-background/80 px-1.5 font-bold font-mono text-[12px]"
            style={{ borderColor: windBand(o.wind).bg }}
          >
            {formatWind(o.wind, unit)}
            {o.gust != null && o.gust > o.wind && (
              <span className="font-medium text-[9px] text-muted-foreground">/{formatWind(o.gust, unit)}</span>
            )}
          </span>
        )}
        {t?.wind != null && (
          <span
            className="inline-flex rounded-md border border-dashed bg-background/80 px-1.5 font-mono font-semibold text-[11px]"
            style={{ borderColor: windBand(t.high ?? t.wind).bg }}
          >
            {t.low !== null && t.high !== null
              ? `${formatWind(t.low, unit)}–${formatWind(t.high, unit)}`
              : formatWind(t.wind, unit)}
          </span>
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

/** "gemessen 17–22 kn": what was measured in the folded hours (their count is on the button) */
function pastSummary(past: Cell[], measured: Measured | undefined, unit: WindUnit): string {
  const winds = measured
    ? past.flatMap((c) => {
        const o = nearest(measured.observations, c.time);
        return o?.wind != null ? [o.wind] : [];
      })
    : [];
  if (winds.length === 0) return '';
  const lo = formatWind(Math.min(...winds), unit);
  const hi = formatWind(Math.max(...winds), unit);
  return m.rows_pastMeasured({ range: lo === hi ? lo : `${lo}–${hi}`, unit: UNIT_LABEL[unit] });
}
