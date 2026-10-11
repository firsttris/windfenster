import { useState } from 'react';
import type { Forecast } from '@/bindings/Forecast';
import type { WindUnit } from '@/bindings/WindUnit';
import type { Day } from '@/lib/forecast';
import { compass, dayMonth, hour, weekday } from '@/lib/format';
import { formatWind, UNIT_LABEL } from '@/lib/units';
import { cn } from '@/lib/utils';
import { m } from '@/paraglide/messages';
import { WindArrow } from '@/ui/wind-arrow';
import { WindCell } from '@/ui/wind-cell';
import { modelName, StageChip, Weather } from './shared';

/** Phone: one day at a time, picked from a row of day chips, as in the mockup. */
export function ForecastList({ fc, days, unit }: { fc: Forecast; days: Day[]; unit: WindUnit }) {
  const [index, setIndex] = useState(0);
  const day = days[Math.min(index, days.length - 1)];
  return (
    <div className="flex flex-col gap-3">
      <fieldset
        aria-label={m.table_days()}
        className="-mx-4 m-0 flex min-w-0 gap-1.5 overflow-x-auto border-0 px-4 py-0 [scrollbar-width:none]"
      >
        {days.map((d, i) => {
          const on = i === index;
          return (
            <button
              key={d.date}
              type="button"
              aria-pressed={on}
              onClick={() => setIndex(i)}
              className={cn(
                'flex h-11 shrink-0 cursor-pointer items-center gap-1.5 rounded-full border px-3.5 font-semibold text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring/60',
                on ? 'border-primary bg-primary text-primary-foreground' : 'bg-background',
                !on && d.stage === 'trend' && 'border-border-strong border-dashed text-muted-foreground',
              )}
            >
              {weekday(d.date, 'short')}
              <span className={cn('font-mono font-medium text-[11px]', on ? 'opacity-70' : 'text-muted-foreground')}>
                {dayMonth(d.date)}
              </span>
            </button>
          );
        })}
      </fieldset>

      <div className={cn('rounded-xl border bg-card px-3 pb-1', day.stage === 'trend' && 'opacity-70')}>
        <div className="flex h-10 items-center justify-between gap-2">
          <StageChip fc={fc} day={day} />
          <span className="font-mono text-[11px] text-muted-foreground">{modelName(fc, day.source)}</span>
        </div>
        <table className="w-full border-collapse text-sm">
          <caption className="sr-only">
            {weekday(day.date, 'long')} {dayMonth(day.date)}
          </caption>
          <thead>
            <tr className="h-[30px] border-line border-b text-[11px] text-muted-foreground">
              <th scope="col" className="w-10 text-left font-normal">
                {m.table_time()}
              </th>
              <th scope="col" className="w-[56px] font-normal">
                {UNIT_LABEL[unit]}
              </th>
              <th scope="col" className="w-11 font-normal">
                {m.table_gusts()}
              </th>
              <th scope="col" className="text-left font-normal">
                {m.table_direction()}
              </th>
              <th scope="col" className="w-9 font-normal">
                <span className="sr-only">{m.table_weather()}</span>
              </th>
            </tr>
          </thead>
          <tbody>
            {day.cells.map((c) => (
              <tr key={c.hour} className={cn('h-12 border-line border-b last:border-b-0', c.past && 'opacity-40')}>
                <th scope="row" className="text-left font-mono font-normal text-[13px] text-muted-foreground">
                  {hour(c.hour)}
                </th>
                <td className="text-center">
                  {c.wind === null ? '–' : <WindCell kn={c.wind} unit={unit} className="mx-auto" />}
                </td>
                <td className="text-center font-mono text-[13px] text-subtle">
                  {c.gust === null ? '–' : formatWind(c.gust, unit)}
                </td>
                <td>
                  {c.dir !== null && (
                    <span className="flex items-center gap-1.5">
                      <WindArrow deg={c.dir} className="size-4" />
                      <span className="font-mono text-muted-foreground text-xs">{compass(c.dir)}</span>
                    </span>
                  )}
                </td>
                <td className="text-center">
                  <Weather cloud={c.cloud} rain={c.rain} />
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
