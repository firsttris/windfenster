import { useState } from 'react';
import type { Forecast } from '@/bindings/Forecast';
import type { GearItem } from '@/bindings/GearItem';
import type { Taf } from '@/bindings/Taf';
import type { WindUnit } from '@/bindings/WindUnit';
import { dayAgreement } from '@/lib/agreement';
import { barShare, type Cell, type Day, isNight } from '@/lib/forecast';
import { compass, dayMonth, formatNumber, hour, weekday } from '@/lib/format';
import { gearFor } from '@/lib/gear';
import type { Measured } from '@/lib/observations';
import { groupsBetween, groupWhat, groupWhen, tafHour } from '@/lib/taf-text';
import { formatWind, UNIT_LABEL } from '@/lib/units';
import { cn } from '@/lib/utils';
import { m } from '@/paraglide/messages';
import { WindArrow } from '@/ui/wind-arrow';
import { windBand } from '@/ui/wind-scale';
import { GearFitCell } from '../gear-fit';
import { Dash, MeasuredCell, TafCell } from './compare-cells';
import { DayRows } from './day-rows';
import { AGREE_DOT, AGREE_LABEL, modelName, StageChip, Weather } from './shared';

interface Props {
  fc: Forecast;
  day: Day;
  unit: WindUnit;
  /** Unix seconds */
  now: number;
  measured?: Measured;
  taf?: Taf | null;
  gear?: readonly GearItem[];
}

/** The wind bar's full height in pixels */
const BAR = 40;

/** One day of the forecast, as its own card: what the model says (wind as a growing bar, the
 * gust set right under it, the direction), then what checks it (measured, the TAF), then the
 * weather. */
export function DayCard({ fc, day, unit, now, measured, taf, gear = [] }: Props) {
  const title = `${weekday(day.date, 'long')}, ${dayMonth(day.date)}`;
  // What was measured belongs to hours that are over; the TAF to the hours it covers.
  const showMeasured = !!measured && day.cells.some((c) => c.past);
  const showTaf = !!taf && day.cells.some((c) => tafHour(taf, c.time)?.wind != null);
  const hourly = day.stage === 'hourly';
  // The "?" of a row opens its sentence under the table: touch screens have no hover
  const [help, setHelp] = useState<'measured' | 'taf' | null>(null);
  const toggle = (row: 'measured' | 'taf') => setHelp((h) => (h === row ? null : row));

  const td = (c: Cell, opts: { fade?: boolean; className?: string } = {}) =>
    cn(
      'p-0 text-center align-middle',
      hourly ? 'min-w-[34px]' : 'min-w-[40px]',
      (opts.fade ?? true) && c.past && 'opacity-40',
      (opts.fade ?? true) && !c.past && !hourly && isNight(c.hour) && 'opacity-55',
      opts.className,
    );

  return (
    <section
      aria-labelledby={`day-${day.date}-title`}
      id={`day-${day.date}`}
      className={cn(
        'min-w-0 scroll-mt-20 rounded-xl border bg-card pt-3 pb-2',
        // Hour by hour a day needs the whole width
        hourly && 'lg:col-span-2',
        day.stage === 'trend' && 'bg-card/60',
      )}
    >
      <header className="flex flex-wrap items-center gap-x-2 gap-y-1.5 px-4 pb-2">
        <h3 id={`day-${day.date}-title`} className="m-0 font-semibold text-[15px]">
          {weekday(day.date, 'long')}
        </h3>
        <span className="font-mono text-muted-foreground text-xs">{dayMonth(day.date)}</span>
        <span className="ml-auto flex flex-wrap items-center gap-1.5">
          <StageChip fc={fc} day={day} />
          <span className="font-mono text-[11px] text-dim">{modelName(fc, day.source)}</span>
          <AgreementChip fc={fc} day={day} />
        </span>
      </header>
      {/* Phones: the hours as rows under one another (day-rows.tsx); wider screens: the table */}
      <DayRows day={day} unit={unit} now={now} measured={measured} taf={taf} gear={gear} className="sm:hidden" />
      <div className="relative hidden overflow-x-auto px-2 sm:block">
        <table className="w-full border-separate border-spacing-0 text-sm">
          <caption className="sr-only">{m.table_dayCaption({ day: title })}</caption>
          <thead>
            <tr>
              <th scope="row" className={label('h-6')}>
                {m.table_time()}
              </th>
              {day.cells.map((c) => (
                <td key={c.hour} className={td(c)}>
                  <span className="font-mono text-[11px] text-muted-foreground">{hour(c.hour)}</span>
                </td>
              ))}
            </tr>
          </thead>
          <tbody>
            <tr>
              {/* The label at the bars' foot, over the gusts: the two read as a pair */}
              <th scope="row" className={label('h-[58px] align-bottom pb-1')}>
                <span className="font-medium text-foreground">{m.table_wind()}</span>
                <span className="ml-1 font-mono text-[11px]">{UNIT_LABEL[unit]}</span>
              </th>
              {day.cells.map((c) => (
                <td key={c.hour} className={td(c, { className: 'align-bottom' })}>
                  {c.wind === null ? <Dash /> : <WindBar kn={c.wind} unit={unit} />}
                </td>
              ))}
            </tr>
            <tr>
              <th scope="row" className={label('h-6')}>
                <span className="font-medium text-foreground">{m.table_gusts()}</span>
              </th>
              {day.cells.map((c) => (
                <td key={c.hour} className={td(c, { className: 'align-top' })}>
                  {c.gust === null ? <Dash /> : <GustCap kn={Math.max(c.gust, c.wind ?? 0)} unit={unit} />}
                </td>
              ))}
            </tr>
            <tr>
              <th scope="row" className={label('h-11')}>
                {m.table_direction()}
              </th>
              {day.cells.map((c) => (
                <td key={c.hour} className={td(c)}>
                  {c.dir === null ? (
                    <Dash />
                  ) : (
                    <span className="flex flex-col items-center gap-0.5 text-foreground">
                      <WindArrow deg={c.dir} />
                      <span className="font-mono text-[10px] text-muted-foreground">{compass(c.dir)}</span>
                    </span>
                  )}
                </td>
              ))}
            </tr>
            {gear.length > 0 && (
              <tr>
                <th scope="row" className={label('h-10')}>
                  {m.table_gear()}
                </th>
                {day.cells.map((c) => (
                  <td key={c.hour} className={td(c)}>
                    <GearFitCell fit={c.wind === null ? null : gearFor(gear, c.wind, c.gust)} />
                  </td>
                ))}
              </tr>
            )}

            {(showMeasured || showTaf) && <GroupRow span={day.cells.length} title={m.table_compare()} />}
            {showMeasured && measured && (
              <tr>
                <th scope="row" className={label('h-9')}>
                  <span className="flex flex-col leading-tight">
                    <span className="flex items-center gap-1 font-semibold text-success-foreground">
                      {m.table_measured()}
                      <Help
                        text={m.table_measuredHelp()}
                        open={help === 'measured'}
                        onToggle={() => toggle('measured')}
                      />
                    </span>
                    <span className="font-mono text-[10px]">{measured.source}</span>
                  </span>
                </th>
                {day.cells.map((c) => (
                  <td key={c.hour} className={td(c, { fade: false })}>
                    <MeasuredCell cell={c} now={now} measured={measured} unit={unit} />
                  </td>
                ))}
              </tr>
            )}
            {showTaf && taf && (
              <tr>
                <th scope="row" className={label('h-9')}>
                  <span className="flex flex-col leading-tight">
                    <span className="flex items-center gap-1 font-semibold text-foreground">
                      {m.taf_row()}
                      <Help text={m.taf_help()} open={help === 'taf'} onToggle={() => toggle('taf')} />
                    </span>
                    <span className="font-mono text-[10px]">{m.taf_rowSub()}</span>
                  </span>
                </th>
                {day.cells.map((c) => (
                  <td key={c.hour} className={td(c, { fade: false })}>
                    <TafCell cell={c} taf={taf} unit={unit} />
                  </td>
                ))}
              </tr>
            )}

            <GroupRow span={day.cells.length} title={m.table_weather()} />
            <tr>
              <th scope="row" className={label('h-8')}>
                <span className="sr-only">{m.table_weather()}</span>
              </th>
              {day.cells.map((c) => (
                <td key={c.hour} className={td(c)}>
                  <Weather cloud={c.cloud} rain={c.rain} />
                </td>
              ))}
            </tr>
            <tr>
              <th scope="row" className={label('h-7')}>
                {m.table_temperature()}
                <span className="ml-1 font-mono text-[11px]">°C</span>
              </th>
              {day.cells.map((c) => (
                <td key={c.hour} className={td(c)}>
                  {c.temp === null ? <Dash /> : <span className="font-mono text-[13px]">{Math.round(c.temp)}°</span>}
                </td>
              ))}
            </tr>
            <tr>
              <th scope="row" className={label('h-7')}>
                {m.table_rain()}
                <span className="ml-1 font-mono text-[11px]">mm</span>
              </th>
              {day.cells.map((c) => (
                <td key={c.hour} className={td(c)}>
                  {c.rain === null ? (
                    <Dash />
                  ) : (
                    <span className={cn('font-mono text-xs', c.rain > 0 ? 'text-water' : 'text-dim')}>
                      {formatNumber(c.rain, c.rain > 0 && c.rain < 10 ? 1 : 0)}
                    </span>
                  )}
                </td>
              ))}
            </tr>
          </tbody>
        </table>
      </div>
      {help && (
        <p role="note" className="m-0 mx-4 mt-2 rounded-lg bg-muted px-3 py-2 text-[12px] text-muted-foreground">
          {help === 'measured' ? m.table_measuredHelp() : m.taf_help()}
        </p>
      )}
      {showTaf && taf && <TafNotes fc={fc} day={day} taf={taf} unit={unit} now={now} />}
    </section>
  );
}

const label = (extra: string) =>
  cn(
    'sticky left-0 z-10 w-[94px] min-w-[94px] max-w-[94px] overflow-hidden bg-card pr-2 pl-2 text-left font-normal text-[12px] text-muted-foreground',
    extra,
  );

/** The wind as a bar that grows with it, in the scale's colour, the value above it */
function WindBar({ kn, unit }: { kn: number; unit: WindUnit }) {
  return (
    <span className="flex flex-col items-center justify-end">
      <span className="mb-0.5 font-bold font-mono text-[13px]">{formatWind(kn, unit)}</span>
      <span
        className="block w-[80%] rounded-t-[4px]"
        style={{ height: Math.max(3, barShare(kn) * BAR), background: windBand(kn).bg }}
      />
    </span>
  );
}

/** The gust, set right under its wind's bar like a base */
function GustCap({ kn, unit }: { kn: number; unit: WindUnit }) {
  const band = windBand(kn);
  return (
    <span
      className="mx-auto block w-[80%] rounded-b-[4px] py-0.5 font-mono font-semibold text-[11px]"
      style={{ background: band.bg, color: band.fg }}
    >
      {formatWind(kn, unit)}
    </span>
  );
}

/** What the TAF says for the day beyond its numbers, in sentences: showers, a temporary gust, a
 * change of wind */
function TafNotes({ fc, day, taf, unit, now }: { fc: Forecast; day: Day; taf: Taf; unit: WindUnit; now: number }) {
  const step = day.stage === 'hourly' ? 3_600 : 3 * 3_600;
  const from = day.cells[0]?.time ?? 0;
  const notes = groupsBetween(taf.groups, from, (day.cells.at(-1)?.time ?? from) + step);
  if (notes.length === 0) return null;
  const timeZone = fc.timezone ?? 'UTC';
  const ctx = { unit, at: taf.issuedAt, timeZone };
  return (
    <dl className="m-0 mx-4 mt-2 grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 border-line border-t pt-2 text-[12px]">
      {notes.map(({ group, prev }) => (
        <div key={`${group.change}-${group.from}`} className="contents">
          <dt className="font-mono text-[11px] text-muted-foreground">
            {m.taf_row()} · {groupWhen(group, now, timeZone)}
          </dt>
          <dd className="m-0 min-w-0 first-letter:uppercase">{groupWhat(group, ctx, prev)}</dd>
        </div>
      ))}
    </dl>
  );
}

/** A heading row inside the table: "Zum Vergleich", "Wetter" */
function GroupRow({ span, title }: { span: number; title: string }) {
  return (
    <tr>
      <td colSpan={span + 1} className="border-line border-t px-2 pt-2 pb-0.5">
        <span className="sticky left-2 font-mono text-[10px] text-dim uppercase tracking-[0.08em]">{title}</span>
      </td>
    </tr>
  );
}

/** A question mark that says in a sentence what a row is: on hover, and under the table when
 * tapped */
function Help({ text, open, onToggle }: { text: string; open: boolean; onToggle: () => void }) {
  return (
    <button
      type="button"
      aria-label={text}
      aria-expanded={open}
      title={text}
      onClick={onToggle}
      className={cn(
        'inline-grid size-3.5 shrink-0 cursor-help place-items-center rounded-full border border-border-strong font-normal font-sans text-[9px] text-muted-foreground',
        open && 'bg-foreground text-background',
      )}
    >
      ?
    </button>
  );
}

/** How far the models agree on the day, in a word, for every day the models overlap */
function AgreementChip({ fc, day }: { fc: Forecast; day: Day }) {
  const a = dayAgreement(
    fc,
    day.cells.map((c) => c.time),
  );
  if (!a) return null;
  return (
    <span className="inline-flex items-center gap-1.5 whitespace-nowrap rounded-full border px-2 py-0.5 text-[11px] text-muted-foreground">
      <span className={cn('size-2 rounded-full', AGREE_DOT[a.level])} aria-hidden="true" />
      {m.agree_day({ level: AGREE_LABEL[a.level]() })}
    </span>
  );
}
