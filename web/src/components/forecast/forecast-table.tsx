import type { Forecast } from '@/bindings/Forecast';
import type { WindUnit } from '@/bindings/WindUnit';
import type { Cell, Day } from '@/lib/forecast';
import { compass, dayMonth, formatNumber, hour, weekday } from '@/lib/format';
import { type Measured, nearest } from '@/lib/observations';
import { formatWind, UNIT_LABEL } from '@/lib/units';
import { cn } from '@/lib/utils';
import { m } from '@/paraglide/messages';
import { WindArrow } from '@/ui/wind-arrow';
import { WindCell } from '@/ui/wind-cell';
import { windBand } from '@/ui/wind-scale';
import { modelName, StageChip, Weather } from './shared';

interface Row {
  label: string;
  sub?: string;
  height: string;
  cell: (c: Cell) => React.ReactNode;
  /** Measured values: label in two lines, past hours not faded. */
  measured?: boolean;
}

interface Props {
  fc: Forecast;
  days: Day[];
  unit: WindUnit;
  /** Unix seconds */
  now: number;
  measured?: Measured;
}

/** Desktop: days side by side, scrolling sideways; the row labels stay in place. */
export function ForecastTable({ fc, days, unit, now, measured }: Props) {
  const rows: Row[] = [
    {
      label: measured ? m.table_forecast() : m.table_wind(),
      sub: UNIT_LABEL[unit],
      height: 'h-[46px]',
      cell: (c) => (c.wind === null ? <Dash /> : <WindCell kn={c.wind} unit={unit} className="mx-auto" />),
    },
    ...(measured
      ? [
          {
            label: m.table_measured(),
            sub: measured.source,
            height: 'h-[38px]',
            measured: true,
            cell: (c: Cell) => <MeasuredCell cell={c} now={now} measured={measured} unit={unit} />,
          },
        ]
      : []),
    {
      label: m.table_gusts(),
      height: 'h-[30px]',
      cell: (c) =>
        c.gust === null ? (
          <Dash />
        ) : (
          <span className="font-mono text-[13px] text-subtle">{formatWind(c.gust, unit)}</span>
        ),
    },
    {
      label: m.table_direction(),
      height: 'h-[46px]',
      cell: (c) =>
        c.dir === null ? (
          <Dash />
        ) : (
          <span className="flex flex-col items-center gap-0.5 text-foreground">
            <WindArrow deg={c.dir} />
            <span className="font-mono text-[10px] text-muted-foreground">{compass(c.dir)}</span>
          </span>
        ),
    },
    { label: m.table_weather(), height: 'h-[38px]', cell: (c) => <Weather cloud={c.cloud} rain={c.rain} /> },
    {
      label: m.table_temperature(),
      sub: '°C',
      height: 'h-[30px]',
      cell: (c) => (c.temp === null ? <Dash /> : <span className="font-mono text-[13px]">{Math.round(c.temp)}°</span>),
    },
    {
      label: m.table_rain(),
      sub: 'mm',
      height: 'h-[30px]',
      cell: (c) =>
        c.rain === null ? (
          <Dash />
        ) : (
          <span className={cn('font-mono text-xs', c.rain > 0 ? 'text-water' : 'text-dim')}>
            {formatNumber(c.rain, c.rain > 0 && c.rain < 10 ? 1 : 0)}
          </span>
        ),
    },
  ];

  const cellClass = (day: Day, i: number, c: Cell, fade = true) =>
    cn(
      'w-[56px] min-w-[56px] p-0 text-center align-middle',
      i === 0 && 'border-l border-line',
      day.stage === 'trend' && 'opacity-60',
      fade && c.past && 'opacity-40',
    );

  return (
    <div className="overflow-x-auto">
      <table className="border-separate border-spacing-0 text-sm">
        <caption className="sr-only">{m.table_caption()}</caption>
        <thead>
          <tr>
            <th scope="col" className="sticky left-0 z-10 w-[116px] min-w-[116px] bg-card" />
            {days.map((d) => (
              <th
                key={d.date}
                scope="colgroup"
                colSpan={d.cells.length}
                className={cn(
                  'border-l border-line px-2 pt-3 pb-2 text-left font-normal',
                  d.stage === 'trend' && 'opacity-60',
                )}
              >
                <span className="flex items-baseline gap-2 whitespace-nowrap">
                  <span className="font-semibold text-sm">{weekday(d.date, 'long')}</span>
                  <span className="font-mono text-muted-foreground text-xs">{dayMonth(d.date)}</span>
                </span>
                <span className="mt-1 flex items-center gap-1.5 whitespace-nowrap">
                  <StageChip fc={fc} day={d} />
                  <span className="font-mono text-[11px] text-dim">{modelName(fc, d.source)}</span>
                </span>
              </th>
            ))}
          </tr>
          <tr>
            <th scope="row" className={labelClass('h-[28px]')}>
              {m.table_time()}
            </th>
            {days.flatMap((d) =>
              d.cells.map((c, i) => (
                <td key={`${d.date}-${c.hour}`} className={cellClass(d, i, c)}>
                  <span className="font-mono text-muted-foreground text-xs">{hour(c.hour)}</span>
                </td>
              )),
            )}
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => (
            <tr key={r.label}>
              <th scope="row" className={labelClass(r.height)}>
                {r.measured ? (
                  <span className="flex flex-col leading-tight">
                    <span className="font-semibold text-success-foreground">{r.label}</span>
                    <span className="font-mono text-[10.5px]">{r.sub}</span>
                  </span>
                ) : (
                  <>
                    <span className="font-medium text-foreground">{r.label}</span>
                    {r.sub && <span className="ml-1.5 font-mono text-[11px] text-muted-foreground">{r.sub}</span>}
                  </>
                )}
              </th>
              {days.flatMap((d) =>
                d.cells.map((c, i) => (
                  <td key={`${d.date}-${c.hour}`} className={cn(cellClass(d, i, c, !r.measured), r.height)}>
                    {r.cell(c)}
                  </td>
                )),
              )}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

const labelClass = (height: string) =>
  cn(
    'sticky left-0 z-10 w-[116px] min-w-[116px] border-r border-line bg-card pl-5 text-left font-normal text-muted-foreground text-[13px]',
    height,
  );

/** The measurement at a past hour, framed in its scale colour; a quiet line for hours to come. */
function MeasuredCell({ cell, now, measured, unit }: { cell: Cell; now: number; measured: Measured; unit: WindUnit }) {
  const o = nearest(measured.observations, cell.time);
  if (o?.wind == null) {
    // Hours to come, and the last ones while their values are on the way (DWD runs about an hour
    // behind), get the quiet line; a dash marks a gap in the measurements.
    const last = measured.observations.at(-1)?.at ?? 0;
    return cell.time > now || cell.time > last ? (
      <span className="mx-auto block h-0.5 w-11 rounded-full bg-line" />
    ) : (
      <Dash />
    );
  }
  return (
    <span
      className="mx-auto flex h-7 w-11 items-center justify-center rounded-[7px] border-2 font-bold font-mono text-sm"
      style={{ borderColor: windBand(o.wind).bg }}
    >
      {formatWind(o.wind, unit)}
    </span>
  );
}

function Dash() {
  return <span className="text-dim">–</span>;
}
