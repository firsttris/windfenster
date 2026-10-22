import { Link } from '@tanstack/react-router';
import { Check, CircleHelp, Info } from 'lucide-react';
import { Fragment } from 'react';
import type { Forecast } from '@/bindings/Forecast';
import type { Model } from '@/bindings/Model';
import type { Ranking } from '@/bindings/Ranking';
import type { WindUnit } from '@/bindings/WindUnit';
import { MIX } from '@/lib/forecast';
import { formatNumber } from '@/lib/format';
import { modelColor } from '@/lib/model-color';
import { byRank, type ChipRank, chipRanks, MIN_DAYS } from '@/lib/ranking';
import { PER_KN, UNIT_LABEL } from '@/lib/units';
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
  /** "Who hits here?": each chip's place and error, and the order within its group */
  ranking?: Ranking | null;
  unit: WindUnit;
  /** The ranking card's station code: the legend of the places links to the card */
  rankingCode?: string;
}

/** The models that cover the place in one row: the fine regional ones (hour by hour), a line,
 * the global ones with the mix (every 3 hours). Each chip shows its place in the station's
 * ranking and its mean error; within a group the chips follow the ranking once it has 14 days,
 * else the fixed order. How the first one was chosen is behind the info
 * mark beside the count, the chosen model's run under it. Narrower than xl the chips show their names only and lines beneath
 * tell the one chosen, its run and what the places are; on phones and tablets the row swipes, from lg on each group is a grid of equal columns. */
export function ModelChips({ fc, selected, onSelect, top, hint, auto, run, ranking, unit, rankingCode }: Props) {
  const ranks = chipRanks(ranking);
  const groups = [
    {
      key: 'super',
      models: byRank(
        fc.models.filter((x) => x.class === 'super'),
        ranks,
      ),
    },
    {
      key: 'global',
      models: byRank(
        fc.models.filter((x) => x.class !== 'super'),
        ranks,
      ),
    },
  ].filter((g) => g.models.length > 0);
  const chosen = fc.models.find((x) => x.id === selected);
  // What the "#" and the error on the chips are, once a chip carries them
  const legend =
    ranking && fc.models.some((x) => ranks.has(x.id))
      ? ranking.days < MIN_DAYS
        ? m.models_legendYoung({ days: ranking.days })
        : m.models_legend()
      : null;
  return (
    <section
      aria-label={m.models_choose()}
      className="flex flex-col gap-3 sm:rounded-xl sm:border sm:bg-card sm:px-[18px] sm:py-4"
    >
      <div className="relative -mx-4 flex items-stretch gap-3.5 overflow-x-auto px-4 [scrollbar-width:none] sm:gap-[22px] lg:mx-0 lg:overflow-visible lg:px-0">
        <div className="hidden min-w-24 shrink-0 flex-col justify-center gap-0.5 xl:flex">
          <span className="flex items-center gap-1.5 font-semibold text-sm">
            {/* Every chip counts, the mix too */}
            {m.models_count({ count: fc.models.length })}
            {hint && <HintMark hint={hint} auto={auto} />}
          </span>
          {run && <span className="whitespace-nowrap font-mono text-[11px] text-muted-foreground">{run}</span>}
          {legend && <Legend legend={legend} code={rankingCode} />}
          <GuideLink model={selected} />
        </div>
        {groups.map((g, i) => (
          <Fragment key={g.key}>
            {i > 0 && <span aria-hidden="true" className="w-px shrink-0 self-stretch bg-border sm:my-0.5" />}
            {/* From lg on the groups share the width, each a grid of equal columns: eleven models
                no longer fit one line, and chips as wide as their text would leave the places and
                errors out of line */}
            <div className="flex shrink-0 items-center gap-1.5 sm:gap-2 lg:grid lg:min-w-0 lg:flex-1 lg:grid-cols-[repeat(auto-fill,minmax(176px,1fr))] lg:content-center lg:gap-y-3 xl:grid-cols-[repeat(auto-fill,minmax(200px,1fr))]">
              {g.models.map((model) => (
                <Chip
                  key={model.id}
                  fc={fc}
                  model={model}
                  on={model.id === selected}
                  top={model.id === top}
                  place={ranks.get(model.id)}
                  unit={unit}
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
            {/* From xl on the run stands left, under the count */}
            {run && <span className="xl:hidden"> · {run}</span>}
          </span>
          {/* The count and its mark stand left from xl on */}
          {hint && <HintMark hint={hint} auto={auto} className="xl:hidden" />}
          <GuideLink model={selected} className="ml-auto shrink-0 xl:hidden" />
        </p>
      )}
      {/* The left column with the legend shows from xl on, beside the chips: narrower it would leave the groups one column each */}
      {legend && <Legend legend={legend} code={rankingCode} className="xl:hidden" />}
    </section>
  );
}

/** What "#" and the error on the chips mean, and a jump to the ranking card they come from */
function Legend({ legend, code, className }: { legend: string; code?: string; className?: string }) {
  return (
    <span
      className={cn(
        'flex flex-wrap items-center gap-x-1.5 whitespace-nowrap text-[11px] text-muted-foreground leading-snug xl:flex-col xl:items-start',
        className,
      )}
    >
      <span>{legend}</span>
      {code && (
        <a
          href="#ranking"
          className="text-foreground underline decoration-line underline-offset-2 hover:decoration-foreground"
        >
          {m.ranking_title({ code })} ↓
        </a>
      )}
    </span>
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
  place,
  unit,
  onSelect,
}: {
  fc: Forecast;
  model: Model;
  on: boolean;
  top: boolean;
  place?: ChipRank;
  unit: WindUnit;
  onSelect: (id: string) => void;
}) {
  const has = fc.series.some((s) => s.model === model.id);
  const mae = place && `${formatNumber(place.mae * PER_KN[unit], 1)} ${UNIT_LABEL[unit]}`;
  const ranked =
    place &&
    mae &&
    (place.settled ? m.models_rank({ rank: place.rank, mae }) : m.models_rankYoung({ rank: place.rank, mae }));
  return (
    <button
      type="button"
      aria-pressed={on}
      disabled={!has}
      title={has ? ranked || undefined : m.models_noData()}
      onClick={() => onSelect(model.id)}
      className={cn(
        // One width for all: below lg a floor the longest name fills, from lg the grid's column
        'relative flex h-11 min-w-36 shrink-0 cursor-pointer items-center gap-[7px] whitespace-nowrap rounded-lg border px-3 text-left outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring/60 disabled:cursor-not-allowed disabled:opacity-40 lg:w-full lg:min-w-0 xl:h-auto xl:min-h-12 xl:gap-2.5 xl:py-1.5',
        on ? 'border-primary bg-primary text-primary-foreground' : 'bg-background hover:border-border-strong',
      )}
    >
      {/* On the chosen chip a ring in its text colour: AROME's dot is the chip's own colour
          (white in dark mode, near-black in light) and vanished there */}
      <span
        className={cn('size-2 shrink-0 rounded-full', on && 'ring-1 ring-primary-foreground')}
        style={{ background: modelColor(fc, model.id) }}
      />
      {/* The name gives way before the place and error: "UKMO Global" with "9,3 km/h" filled a
          chip from lg at 1024 px and pushed them past its edge */}
      <span className="flex min-w-0 flex-col">
        <span className="truncate font-semibold text-sm leading-tight">{modelName(fc, model.id)}</span>
        <span className={cn('hidden font-mono text-[11px] xl:block', on ? 'opacity-70' : 'text-muted-foreground')}>
          {details(model)}
        </span>
      </span>
      {/* Its place in "who hits here?" and the mean error at the right edge, so they stand in
          line chip under chip; paler while under 14 days */}
      {place && (
        <span
          className={cn(
            'ml-auto flex shrink-0 flex-col items-end font-mono leading-tight',
            !place.settled && 'opacity-55',
          )}
        >
          <span className={cn('text-[10px]', on ? 'opacity-70' : 'text-muted-foreground')}>#{place.rank}</span>
          <span className="font-semibold text-[11px]">{mae}</span>
        </span>
      )}
      {/* On the chip's top edge, so the best chip is no wider than the others; below lg the
          row scrolls and would clip it, and the line under the chips says it */}
      {top && (
        <span
          className={cn(
            'absolute -top-2 left-3 hidden whitespace-nowrap rounded-full border px-1.5 font-semibold text-[10px] leading-[14px] lg:inline',
            on
              ? 'border-primary bg-primary-foreground text-primary'
              : 'border-success-border bg-success-surface text-success-foreground',
          )}
        >
          {m.models_top()}
        </span>
      )}
    </button>
  );
}
