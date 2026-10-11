import { Check } from 'lucide-react';
import type { Forecast } from '@/bindings/Forecast';
import { MIX } from '@/lib/forecast';
import { formatNumber } from '@/lib/format';
import { cn } from '@/lib/utils';
import { m } from '@/paraglide/messages';
import { modelColor, modelName } from './shared';

/** The models that cover the place; the chosen one is highlighted. */
export function ModelChips({
  fc,
  selected,
  onSelect,
  top,
  hint,
  auto,
}: {
  fc: Forecast;
  selected: string | null;
  onSelect: (id: string) => void;
  /** The model that hits best here, marked "Best here" */
  top: string | null;
  /** How the first model was chosen */
  hint: string;
  /** The choice comes from the station's ranking */
  auto: boolean;
}) {
  return (
    <section
      aria-label={m.models_choose()}
      className="flex items-center gap-3.5 sm:flex-wrap sm:rounded-xl sm:border sm:bg-card sm:p-3.5"
    >
      <div className="hidden min-w-[110px] flex-col gap-0.5 px-1.5 sm:flex">
        <span className="font-medium text-[11px] text-muted-foreground uppercase tracking-[0.1em]">
          {m.models_source()}
        </span>
        <span className="font-semibold text-sm">
          {m.models_count({ count: fc.models.filter((x) => x.id !== MIX).length })}
        </span>
      </div>
      <div className="-mx-4 flex min-w-0 flex-1 gap-2 overflow-x-auto px-4 [scrollbar-width:none] sm:mx-0 sm:flex-wrap sm:overflow-visible sm:px-0">
        {fc.models.map((model) => {
          const on = model.id === selected;
          const has = fc.series.some((s) => s.model === model.id);
          return (
            <button
              key={model.id}
              type="button"
              aria-pressed={on}
              disabled={!has}
              title={has ? undefined : m.models_noData()}
              onClick={() => onSelect(model.id)}
              className={cn(
                'flex min-h-11 shrink-0 cursor-pointer items-center gap-2.5 rounded-lg border px-3 py-1.5 text-left outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring/60 disabled:cursor-not-allowed disabled:opacity-40',
                on ? 'border-primary bg-primary text-primary-foreground' : 'bg-background hover:border-border-strong',
              )}
            >
              <span className="size-2 shrink-0 rounded-full" style={{ background: modelColor(fc, model.id) }} />
              <span className="flex flex-col">
                <span className="font-semibold text-sm leading-tight">{modelName(fc, model.id)}</span>
                <span className={cn('font-mono text-[11px]', on ? 'opacity-70' : 'text-muted-foreground')}>
                  {model.id === MIX
                    ? m.mix_sub({ days: Math.min(10, Math.round(model.maxLeadH / 24)) })
                    : `${formatNumber(model.resolutionKm, Number.isInteger(model.resolutionKm) ? 0 : 1)} km · ${m.models_days({ days: Math.round(model.maxLeadH / 24) })}`}
                </span>
              </span>
              {model.id === top && (
                <span
                  className={cn(
                    'ml-1 whitespace-nowrap rounded-full px-2 py-0.5 font-semibold text-[11px]',
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
        })}
      </div>
      {auto ? (
        <p className="m-0 hidden max-w-[340px] items-center gap-2.5 rounded-lg border border-success-border bg-success-surface px-3.5 py-2.5 text-[13px] text-success-foreground leading-snug lg:flex">
          <Check className="size-[18px] shrink-0 text-success" strokeWidth={2.4} aria-hidden="true" />
          <span>{hint}</span>
        </p>
      ) : (
        <p className="m-0 hidden max-w-xs text-muted-foreground text-xs leading-snug lg:block">{hint}</p>
      )}
    </section>
  );
}
