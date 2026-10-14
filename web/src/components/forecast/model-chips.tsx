import { Check } from 'lucide-react';
import { Fragment } from 'react';
import type { Forecast } from '@/bindings/Forecast';
import type { Model } from '@/bindings/Model';
import { MIX } from '@/lib/forecast';
import { formatNumber } from '@/lib/format';
import { cn } from '@/lib/utils';
import { m } from '@/paraglide/messages';
import { modelColor, modelName } from './shared';

interface Props {
  fc: Forecast;
  selected: string | null;
  onSelect: (id: string) => void;
  /** The model that hits best here, marked "Best here" */
  top: string | null;
  /** How the first model was chosen */
  hint: string;
  /** The choice comes from the station's ranking */
  auto: boolean;
}

/** The models that cover the place, as in the mockup: two groups side by side in one row, a
 * line between them, the fine regional ones (hour by hour) and the global ones with the mix
 * (every 3 hours). Each group's title says so, with ticks as dense as its steps. Narrower than
 * xl the chips show their names only and a line beneath tells the one chosen; on phones and
 * tablets the row swipes. */
export function ModelChips({ fc, selected, onSelect, top, hint, auto }: Props) {
  const groups = [
    { title: m.models_rowSuper(), ticks: 13, models: fc.models.filter((x) => x.class === 'super') },
    { title: m.models_rowGlobal(), ticks: 5, models: fc.models.filter((x) => x.class !== 'super') },
  ].filter((g) => g.models.length > 0);
  const chosen = fc.models.find((x) => x.id === selected);
  return (
    <section
      aria-label={m.models_choose()}
      className="flex flex-col gap-3 sm:rounded-xl sm:border sm:bg-card sm:px-[18px] sm:py-4"
    >
      <div className="relative -mx-4 flex items-stretch gap-3.5 overflow-x-auto px-4 [scrollbar-width:none] sm:gap-[22px] lg:mx-0 lg:overflow-visible lg:px-0">
        <div className="hidden min-w-24 shrink-0 flex-col justify-center gap-0.5 lg:flex">
          <span className="font-medium text-[11px] text-muted-foreground uppercase tracking-[0.1em]">
            {m.models_source()}
          </span>
          <span className="font-semibold text-sm">
            {m.models_count({ count: fc.models.filter((x) => x.id !== MIX).length })}
          </span>
        </div>
        {groups.map((g, i) => (
          <Fragment key={g.title}>
            {i > 0 && <span aria-hidden="true" className="w-px shrink-0 self-stretch bg-border sm:my-0.5" />}
            <div className="flex shrink-0 flex-col gap-[7px] sm:gap-2">
              <span className="flex items-center gap-1.5 whitespace-nowrap font-mono text-[10px] text-muted-foreground uppercase tracking-[0.08em] sm:gap-2 sm:text-[11px]">
                <Ticks count={g.ticks} />
                {g.title}
              </span>
              <div className="flex gap-1.5 sm:gap-2">
                {g.models.map((model) => (
                  <Chip
                    key={model.id}
                    fc={fc}
                    model={model}
                    on={model.id === selected}
                    top={model.id === top}
                    onSelect={onSelect}
                  />
                ))}
              </div>
            </div>
          </Fragment>
        ))}
      </div>
      {chosen && (
        <p className="m-0 text-muted-foreground text-xs leading-snug xl:hidden">
          <b className="font-semibold text-foreground">{modelName(fc, chosen.id)}</b> · {details(chosen)}
          {chosen.id === top && <> · {m.models_top()}</>}
        </p>
      )}
      {auto ? (
        <p className="m-0 hidden items-center gap-2 border-t pt-2.5 text-success-foreground text-xs leading-snug sm:flex">
          <Check className="size-4 shrink-0 text-success" strokeWidth={2.4} aria-hidden="true" />
          <span>{hint}</span>
        </p>
      ) : (
        <p className="m-0 hidden border-t pt-2.5 text-muted-foreground text-xs leading-snug sm:block">{hint}</p>
      )}
    </section>
  );
}

/** "2,2 km · 2 Tage", or for the mix "gewichtet · 10 Tage" */
function details(model: Model): string {
  return model.id === MIX
    ? m.mix_sub({ days: Math.min(10, Math.round(model.maxLeadH / 24)) })
    : `${formatNumber(model.resolutionKm, Number.isInteger(model.resolutionKm) ? 0 : 1)} km · ${m.models_days({ days: Math.round(model.maxLeadH / 24) })}`;
}

/** A group's step as ticks: dense for hourly, sparse for every 3 hours */
function Ticks({ count }: { count: number }) {
  const w = 40;
  return (
    <svg aria-hidden="true" viewBox={`0 0 ${w} 10`} className="h-[9px] w-[27px] shrink-0 sm:h-2.5 sm:w-10">
      <path
        d={Array.from({ length: count }, (_, i) => `M${1 + (i * (w - 2)) / (count - 1)} 1v8`).join('')}
        stroke="currentColor"
        strokeWidth={1.4}
      />
    </svg>
  );
}

function Chip({
  fc,
  model,
  on,
  top,
  onSelect,
}: {
  fc: Forecast;
  model: Model;
  on: boolean;
  top: boolean;
  onSelect: (id: string) => void;
}) {
  const has = fc.series.some((s) => s.model === model.id);
  return (
    <button
      type="button"
      aria-pressed={on}
      disabled={!has}
      title={has ? undefined : m.models_noData()}
      onClick={() => onSelect(model.id)}
      className={cn(
        'flex h-11 shrink-0 cursor-pointer items-center gap-[7px] whitespace-nowrap rounded-lg border px-3 text-left outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring/60 disabled:cursor-not-allowed disabled:opacity-40 xl:h-auto xl:min-h-12 xl:gap-2.5 xl:py-1.5',
        on ? 'border-primary bg-primary text-primary-foreground' : 'bg-background hover:border-border-strong',
      )}
    >
      <span className="size-2 shrink-0 rounded-full" style={{ background: modelColor(fc, model.id) }} />
      <span className="flex flex-col">
        <span className="font-semibold text-sm leading-tight">{modelName(fc, model.id)}</span>
        <span className={cn('hidden font-mono text-[11px] xl:block', on ? 'opacity-70' : 'text-muted-foreground')}>
          {details(model)}
        </span>
      </span>
      {top && (
        <span
          className={cn(
            'ml-0.5 hidden whitespace-nowrap rounded-full px-2 py-0.5 font-semibold text-[11px] xl:inline',
            on
              ? 'bg-primary-foreground text-primary'
              : 'border border-success-border bg-success-surface text-success-foreground',
          )}
        >
          {m.models_top()}
        </span>
      )}
    </button>
  );
}
