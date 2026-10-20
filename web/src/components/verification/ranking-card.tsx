import type { Forecast } from '@/bindings/Forecast';
import type { Ranking } from '@/bindings/Ranking';
import { modelName } from '@/components/forecast/shared';
import { MIX } from '@/lib/forecast';
import { formatNumber } from '@/lib/format';
import { modelColor } from '@/lib/model-color';
import { useUnit } from '@/lib/prefs';
import { KMH_PER_KN, MS_PER_KN, UNIT_LABEL } from '@/lib/units';
import { cn } from '@/lib/utils';
import { m } from '@/paraglide/messages';
import { Card } from '@/ui/card';

const REPO = 'https://github.com/firsttris/windfenster#how-models-are-verified';
const MIN_DAYS = 14;

/** "Who hits here?": the mean error of every model, the TAF and the mix over the last 30
 * days, first day ahead. A row picks its model. */
export function RankingCard({
  fc,
  ranking,
  code,
  source,
  from,
  selected,
  onSelect,
}: {
  fc: Forecast;
  ranking: Ranking;
  /** The station's code for the title */
  code: string;
  /** The measurements', e.g. "METAR EDSB" */
  source: string;
  /** Points: where the ranking comes from */
  from?: string;
  selected: string | null;
  onSelect: (id: string) => void;
}) {
  const [unit] = useUnit();
  const factor = unit === 'kn' ? 1 : unit === 'ms' ? MS_PER_KN : KMH_PER_KN;
  const hours = Math.max(0, ...ranking.scores.map((s) => s.n));
  const sub = (id: string) => {
    if (id === 'taf') return m.ranking_tafSub();
    if (id === MIX) return m.ranking_mixSub();
    const model = fc.models.find((x) => x.id === id);
    return model ? `${formatNumber(model.resolutionKm, Number.isInteger(model.resolutionKm) ? 0 : 1)} km` : '';
  };
  return (
    <Card aria-labelledby="ranking-title" className="flex-[1_1_340px] gap-3">
      <div>
        <h2 id="ranking-title" className="m-0 font-semibold text-lg tracking-[-0.02em]">
          {m.ranking_title({ code })}
        </h2>
        <p className="m-0 mt-1 text-[13px] text-muted-foreground">{from ?? m.ranking_sub({ source })}</p>
      </div>
      {ranking.scores.length === 0 ? (
        <p className="m-0 text-muted-foreground text-sm">{m.ranking_empty()}</p>
      ) : (
        <ol className="m-0 flex list-none flex-col gap-1 p-0">
          {ranking.scores.map((s, i) => {
            const pickable = s.model !== 'taf' && fc.series.some((x) => x.model === s.model);
            const on = s.model === selected;
            const row = (
              <>
                <span className="w-3.5 font-mono text-muted-foreground text-xs">{i + 1}</span>
                <span className="flex w-[104px] shrink-0 flex-col items-start gap-px">
                  <span className="font-semibold text-sm">{modelName(fc, s.model)}</span>
                  <span className="font-mono text-[11px] text-muted-foreground">{sub(s.model)}</span>
                </span>
                <span className="block h-2 flex-1 overflow-hidden rounded-full bg-line">
                  <span
                    className="block h-full rounded-full"
                    style={{
                      background: modelColor(fc, s.model),
                      width: `${Math.max(4, Math.min(100, (1 - s.mae / 5) * 100))}%`,
                    }}
                  />
                </span>
                <span className="w-[68px] text-right font-mono font-semibold text-[13px]">
                  {formatNumber(s.mae * factor, 1)} {UNIT_LABEL[unit]}
                </span>
              </>
            );
            const cls = cn(
              'flex min-h-[52px] w-full items-center gap-3 rounded-[10px] border px-3 py-2 text-left',
              on ? 'border-border-strong bg-secondary' : 'border-transparent',
            );
            return (
              <li key={s.model}>
                {pickable ? (
                  <button
                    type="button"
                    aria-pressed={on}
                    onClick={() => onSelect(s.model)}
                    className={cn(
                      cls,
                      'cursor-pointer outline-none hover:bg-secondary focus-visible:ring-2 focus-visible:ring-ring/60',
                    )}
                  >
                    {row}
                  </button>
                ) : (
                  <div className={cls}>{row}</div>
                )}
              </li>
            );
          })}
        </ol>
      )}
      {ranking.scores.length > 0 && ranking.days < MIN_DAYS && (
        <p className="m-0 text-muted-foreground text-xs">{m.ranking_young({ days: ranking.days })}</p>
      )}
      <div className="mt-auto flex flex-wrap items-center justify-between gap-3 border-t pt-3 text-muted-foreground text-xs">
        <span>{m.ranking_hours({ source, hours })}</span>
        <a href={REPO} className="font-medium text-subtle hover:text-foreground">
          {m.ranking_method()} →
        </a>
      </div>
    </Card>
  );
}
