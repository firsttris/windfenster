import { Check } from 'lucide-react';
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

/** The models that cover the place, in two rows: the fine regional ones (hour by hour), then the
 * global ones and the mix (every 3 hours, ten days). The row's title says what its models are,
 * and the model chosen decides the step of the days below. */
export function ModelChips({ fc, selected, onSelect, top, hint, auto }: Props) {
  const rows = [
    { title: m.models_rowSuper(), models: fc.models.filter((x) => x.class === 'super') },
    { title: m.models_rowGlobal(), models: fc.models.filter((x) => x.class !== 'super') },
  ].filter((r) => r.models.length > 0);
  return (
    <section aria-label={m.models_choose()} className="flex flex-col gap-3 sm:rounded-xl sm:border sm:bg-card sm:p-3.5">
      {rows.map((r) => (
        <div key={r.title} className="flex flex-col gap-1.5">
          <span className="font-mono text-[11px] text-muted-foreground uppercase tracking-[0.08em]">{r.title}</span>
          <div className="relative -mx-4 flex min-w-0 gap-2 overflow-x-auto px-4 [scrollbar-width:none] sm:mx-0 sm:flex-wrap sm:overflow-visible sm:px-0">
            {r.models.map((model) => (
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
      ))}
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
      {top && (
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
}
