import { Link } from '@tanstack/react-router';
import { Check, CircleHelp, Info } from 'lucide-react';
import { Fragment } from 'react';
import type { Forecast } from '@/bindings/Forecast';
import type { Model } from '@/bindings/Model';
import { MIX } from '@/lib/forecast';
import { formatNumber } from '@/lib/format';
import { modelColor } from '@/lib/model-color';
import { cn } from '@/lib/utils';
import { m } from '@/paraglide/messages';
import { modelName } from './shared';

interface Props {
  fc: Forecast;
  selected: string | null;
  onSelect: (id: string) => void;
  /** The model that hits best here, marked "Best here" */
  top: string | null;
  /** How the first model was chosen; none when the saved spot's starting model was */
  hint?: string;
  /** The choice comes from the station's ranking */
  auto: boolean;
  /** The chosen model's run and how long ago it came ("Lauf 12 UTC · vor 6 Min.") */
  run?: string;
}

/** The models that cover the place in one row: the fine regional ones (hour by hour), a line,
 * the global ones with the mix (every 3 hours). How the first one was chosen is behind the info
 * mark beside the count, the chosen model's run under it. Narrower than xl the chips show their names only and a line beneath
 * tells the one chosen; on phones and tablets the row swipes, from lg on each group wraps in its own column. */
export function ModelChips({ fc, selected, onSelect, top, hint, auto, run }: Props) {
  const groups = [
    { key: 'super', models: fc.models.filter((x) => x.class === 'super') },
    { key: 'global', models: fc.models.filter((x) => x.class !== 'super') },
  ].filter((g) => g.models.length > 0);
  const chosen = fc.models.find((x) => x.id === selected);
  return (
    <section
      aria-label={m.models_choose()}
      className="flex flex-col gap-3 sm:rounded-xl sm:border sm:bg-card sm:px-[18px] sm:py-4"
    >
      <div className="relative -mx-4 flex items-stretch gap-3.5 overflow-x-auto px-4 [scrollbar-width:none] sm:gap-[22px] lg:mx-0 lg:overflow-visible lg:px-0">
        <div className="hidden min-w-24 shrink-0 flex-col justify-center gap-0.5 lg:flex">
          <span className="flex items-center gap-1.5 font-semibold text-sm">
            {/* Every chip counts, the mix too */}
            {m.models_count({ count: fc.models.length })}
            {hint && <HintMark hint={hint} auto={auto} />}
          </span>
          {run && <span className="whitespace-nowrap font-mono text-[11px] text-muted-foreground">{run}</span>}
          <GuideLink model={selected} />
        </div>
        {groups.map((g, i) => (
          <Fragment key={g.key}>
            {i > 0 && <span aria-hidden="true" className="w-px shrink-0 self-stretch bg-border sm:my-0.5" />}
            {/* From lg on the groups share the width and wrap: eleven models no longer fit one line */}
            <div className="flex shrink-0 items-center gap-1.5 sm:gap-2 lg:min-w-0 lg:shrink lg:flex-wrap lg:content-center">
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
          </Fragment>
        ))}
      </div>
      {chosen && (
        <p className="m-0 flex items-center gap-1.5 text-muted-foreground text-xs leading-snug xl:hidden">
          <span>
            <b className="font-semibold text-foreground">{modelName(fc, chosen.id)}</b> · {details(chosen)}
            {chosen.id === top && <> · {m.models_top()}</>}
            {/* From lg on the run stands left, under the count */}
            {run && <span className="lg:hidden"> · {run}</span>}
          </span>
          {/* The count and its mark stand left from lg on */}
          {hint && <HintMark hint={hint} auto={auto} className="lg:hidden" />}
          <GuideLink model={selected} className="ml-auto shrink-0 lg:hidden" />
        </p>
      )}
    </section>
  );
}

/** To the models page, at the chosen model's card */
function GuideLink({ model, className }: { model: string | null; className?: string }) {
  return (
    <Link
      to="/models"
      hash={model && model !== MIX ? model : undefined}
      className={cn(
        'inline-flex w-fit items-center gap-1 whitespace-nowrap text-[11px] text-muted-foreground underline decoration-line underline-offset-2 hover:text-foreground',
        className,
      )}
    >
      <CircleHelp aria-hidden="true" className="size-3" />
      {m.guide_link()}
    </Link>
  );
}

/** How the first model was chosen, on hover: a tick when the station's ranking chose it. */
function HintMark({ hint, auto, className }: { hint: string; auto: boolean; className?: string }) {
  return (
    <span role="img" aria-label={hint} title={hint} className={cn('cursor-help', className)}>
      {auto ? (
        <Check className="size-3.5 text-success" strokeWidth={2.4} aria-hidden="true" />
      ) : (
        <Info className="size-3.5 text-muted-foreground" aria-hidden="true" />
      )}
    </span>
  );
}

/** "2,2 km · 2 Tage", or for the mix "gewichtet · 10 Tage" */
function details(model: Model): string {
  return model.id === MIX
    ? m.mix_sub({ days: Math.min(10, Math.round(model.maxLeadH / 24)) })
    : `${formatNumber(model.resolutionKm, Number.isInteger(model.resolutionKm) ? 0 : 1)} km · ${m.models_days({ days: Math.round(model.maxLeadH / 24) })}`;
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
      {/* On the chosen chip a ring in its text colour: AROME's dot is the chip's own colour
          (white in dark mode, near-black in light) and vanished there */}
      <span
        className={cn('size-2 shrink-0 rounded-full', on && 'ring-1 ring-primary-foreground')}
        style={{ background: modelColor(fc, model.id) }}
      />
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
