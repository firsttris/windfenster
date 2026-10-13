/** The ten days at a glance, above the day cards: per day its wind every 3 hours as growing bars
 * with the gust as a tick, the main direction and the day's strongest wind, how far the models
 * agree, and for a saved spot the hours that work there. A day leads to its card. */
import type { Forecast } from '@/bindings/Forecast';
import type { WindUnit } from '@/bindings/WindUnit';
import { dayAgreement } from '@/lib/agreement';
import type { DayGlance } from '@/lib/day-glance';
import { barShare, type Day } from '@/lib/forecast';
import { compass, dayMonth, weekday } from '@/lib/format';
import { formatWind, UNIT_LABEL } from '@/lib/units';
import { cn } from '@/lib/utils';
import { m } from '@/paraglide/messages';
import { WindArrow } from '@/ui/wind-arrow';
import { windBand } from '@/ui/wind-scale';
import { AGREE_DOT, AGREE_LABEL } from './shared';

interface Props {
  fc: Forecast;
  /** The days every 3 hours, for the agreement */
  days: Day[];
  glances: DayGlance[];
  unit: WindUnit;
  /** The saved spot's name, when the hours that work there are marked */
  spot?: string;
}

/** The bars' full height in pixels */
const BAR = 44;

export function QuickView({ fc, days, glances, unit, spot }: Props) {
  return (
    <section aria-labelledby="glance-title" className="flex min-w-0 flex-col gap-2">
      <div className="flex flex-wrap items-baseline justify-between gap-x-3 gap-y-1 px-1">
        <h3 id="glance-title" className="m-0 font-semibold text-[15px]">
          {m.glance_title()}
        </h3>
        <span className="flex items-center gap-1.5 text-muted-foreground text-xs">
          {spot ? (
            <>
              <span className="h-1 w-3 rounded-full bg-success" aria-hidden="true" />
              {m.glance_works({ spot })}
            </>
          ) : (
            m.glance_note()
          )}
        </span>
      </div>
      <ol className="relative m-0 flex list-none gap-1.5 overflow-x-auto p-0 pb-1 lg:grid lg:grid-cols-10 lg:overflow-visible">
        {glances.map((g) => {
          const day = days.find((d) => d.date === g.date);
          return (
            <li key={g.date} className="min-w-[84px] flex-1">
              <DayButton fc={fc} day={day} glance={g} unit={unit} />
            </li>
          );
        })}
      </ol>
    </section>
  );
}

function DayButton({ fc, day, glance: g, unit }: { fc: Forecast; day?: Day; glance: DayGlance; unit: WindUnit }) {
  const jump = () => {
    const reduce = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    document.getElementById(`day-${g.date}`)?.scrollIntoView({ behavior: reduce ? 'auto' : 'smooth', block: 'start' });
  };
  const band = g.max === null ? null : windBand(g.max);
  return (
    <button
      type="button"
      onClick={jump}
      title={m.glance_jump({ day: `${weekday(g.date, 'long')}, ${dayMonth(g.date)}` })}
      className={cn(
        'flex h-full w-full cursor-pointer flex-col gap-1.5 rounded-xl border bg-card px-2 pt-2 pb-2 text-left transition-colors hover:border-border-strong focus-visible:outline-2 focus-visible:outline-ring',
        g.works && 'border-success-border bg-success-surface',
      )}
    >
      <span className="flex flex-col whitespace-nowrap leading-tight">
        <span className="font-semibold text-[13px]">
          <span aria-hidden="true">{weekday(g.date, 'short')}</span>
          <span className="sr-only">{weekday(g.date, 'long')}</span>
        </span>
        <span className="font-mono text-[10px] text-muted-foreground">{dayMonth(g.date)}</span>
      </span>

      <span className="flex items-end gap-px" style={{ height: BAR }} aria-hidden="true">
        {g.bars.map((b) => (
          <span key={b.hour} className={cn('relative flex h-full flex-1 items-end', b.past && 'opacity-35')}>
            {/* The gust as a tick above the bar */}
            <span
              className="absolute inset-x-0 h-[2px] rounded-full"
              style={{ bottom: Math.max(3, barShare(b.gust) * BAR) - 2, background: windBand(b.gust).bg }}
            />
            <span
              className={cn('block w-full rounded-t-[2px]', b.night && !b.past && 'opacity-45')}
              style={{ height: Math.max(3, barShare(b.wind) * BAR), background: windBand(b.wind).bg }}
            />
          </span>
        ))}
      </span>
      {/* The hours that work at the spot */}
      {g.works && (
        <span className="-mt-1 flex gap-px" aria-hidden="true">
          {g.bars.map((b) => (
            <span key={b.hour} className={cn('h-1 flex-1 rounded-full', b.works ? 'bg-success' : 'bg-transparent')} />
          ))}
        </span>
      )}

      <span className="flex items-center justify-between gap-1">
        {g.main === null ? (
          <span className="text-dim text-xs">–</span>
        ) : (
          <span className="flex items-center gap-0.5 text-foreground">
            <WindArrow deg={g.main} className="size-[15px]" />
            <span className="font-mono text-[10px] text-muted-foreground">{compass(g.main)}</span>
          </span>
        )}
      </span>
      {day && <Agreement fc={fc} day={day} />}
      {g.max !== null && band && (
        <span
          className="self-start rounded-[5px] px-1.5 py-0.5 font-mono font-semibold text-[11px]"
          style={{ background: band.bg, color: band.fg }}
        >
          {m.glance_max({ wind: formatWind(g.max, unit), unit: UNIT_LABEL[unit] })}
        </span>
      )}
      {g.works && <span className="sr-only">{m.glance_dayWorks()}</span>}
    </button>
  );
}

/** How far the models agree on the day: a dot and a word */
function Agreement({ fc, day }: { fc: Forecast; day: Day }) {
  const a = dayAgreement(
    fc,
    day.cells.map((c) => c.time),
  );
  if (!a) return null;
  return (
    <span
      className="flex items-center gap-1 text-[10px] text-muted-foreground"
      title={m.agree_day({ level: AGREE_LABEL[a.level]() })}
    >
      <span className={cn('size-1.5 shrink-0 rounded-full', AGREE_DOT[a.level])} aria-hidden="true" />
      <span className="sr-only">{m.glance_agree()}</span>
      {AGREE_LABEL[a.level]()}
    </span>
  );
}
