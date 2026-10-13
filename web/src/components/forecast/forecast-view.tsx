import { useQuery } from '@tanstack/react-query';
import { Link } from '@tanstack/react-router';
import { TriangleAlert } from 'lucide-react';
import { type ReactNode, useMemo, useState } from 'react';
import type { Forecast } from '@/bindings/Forecast';
import type { Ranking } from '@/bindings/Ranking';
import type { Taf } from '@/bindings/Taf';
import { AgreementTile } from '@/components/verification/agreement-tile';
import { CompareChart } from '@/components/verification/compare-chart';
import { RankingCard } from '@/components/verification/ranking-card';
import { favoritesQuery, meQuery } from '@/lib/api';
import { dayGlance, spotWorks } from '@/lib/day-glance';
import { type FavoriteTarget, findFavorite } from '@/lib/favorites';
import { autoModel, buildDays, initialModel, MIX, modeOf } from '@/lib/forecast';
import { ago, runHour } from '@/lib/format';
import { noticeText } from '@/lib/i18n';
import type { Measured } from '@/lib/observations';
import { useUnit } from '@/lib/prefs';
import { UNIT_LABEL, WIND_UNITS } from '@/lib/units';
import { useGear } from '@/lib/use-gear';
import { useNow } from '@/lib/use-now';
import { m } from '@/paraglide/messages';
import { Segmented } from '@/ui/segmented';
import { ForecastDays } from './forecast-days';
import { ModelChips } from './model-chips';
import { QuickView } from './quick-view';
import { modelName, ScaleLegend } from './shared';

interface Props {
  fc: Forecast;
  /** Cards between the models and the table, e.g. the live measurement. */
  tiles?: ReactNode;
  /** Measured values, to check the forecast against */
  measured?: Measured;
  /** The station's TAF, for its row */
  taf?: Taf | null;
  /** "Who hits here?": the station's, or for a point the nearest station's within 5 km */
  ranking?: Ranking | null;
  /** The station's measurements over the day, below the table */
  history?: ReactNode;
  /** The station the ranking belongs to: its code, and for points how far away it is */
  rankingStation?: { code: string; source: string; from?: string };
  /** The page as a favourite: when it is one, the quick view marks the hours that work there */
  spot?: FavoriteTarget;
}

/** Everything below a place's title: mode and unit, the models, tiles, the ten days. */
export function ForecastView({ fc, tiles, measured, taf, ranking, rankingStation, history, spot }: Props) {
  const [unit, setUnit] = useUnit();
  const [picked, setPicked] = useState<string | null>(null);
  const globalModel = autoModel(fc, ranking, 'global');
  const selected = picked && fc.series.some((s) => s.model === picked) ? picked : initialModel(fc, ranking);
  const now = useNow();
  const { gear } = useGear();
  // A fine (super) model hour by hour, a global one every 3 hours: the model chosen decides
  const mode = selected ? modeOf(fc, selected) : null;
  const hourly = mode !== 'global';
  const days = useMemo(
    () => (selected ? buildDays(fc, selected, now, globalModel, hourly) : []),
    [fc, selected, now, globalModel, hourly],
  );
  // The quick view always in 3 hours, the same columns for all ten days
  const threeHourly = useMemo(
    () => (selected ? buildDays(fc, selected, now, globalModel, false) : []),
    [fc, selected, now, globalModel],
  );
  const me = useQuery(meQuery);
  const favorites = useQuery({ ...favoritesQuery, enabled: Boolean(me.data) });
  const fav = spot && favorites.data ? findFavorite(favorites.data, spot) : undefined;
  const glances = useMemo(() => {
    const works = fav ? spotWorks(fav.sector, gear) : null;
    return threeHourly.map((d) => dayGlance(d, works));
  }, [threeHourly, fav, gear]);
  const series = fc.series.find((s) => s.model === selected);
  const top = ranking?.mixFirst ? MIX : (ranking?.bestSuper ?? ranking?.bestGlobal ?? null);
  const hint = ranking?.mixFirst
    ? m.models_autoMix()
    : top
      ? m.models_auto({ model: modelName(fc, top) })
      : ranking
        ? m.models_collecting({ days: ranking.days })
        : m.models_noStation();

  return (
    <div className="flex flex-col gap-4">
      {/* What is measured now first, then the run, the unit and the models right above the days */}
      {tiles && <div className="grid items-start gap-4 md:grid-cols-2 xl:grid-cols-3">{tiles}</div>}

      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex w-full items-center justify-between gap-3 sm:w-auto">
          {series && (
            <span className="font-mono text-muted-foreground text-xs">
              {m.forecast_run({ run: runHour(series.runAt), age: ago(series.fetchedAt, now) })}
            </span>
          )}
          <Segmented
            label={m.scale_unit()}
            value={unit}
            options={WIND_UNITS.map((u) => ({ value: u, label: UNIT_LABEL[u] }))}
            onChange={setUnit}
          />
        </div>
      </div>

      {fc.models.length > 0 && (
        <ModelChips fc={fc} selected={selected} onSelect={setPicked} top={top} hint={hint} auto={!!top} />
      )}

      {fc.notice && (
        <p
          role="status"
          className="m-0 flex items-start gap-2.5 rounded-xl border border-warning-border bg-warning-surface px-4 py-3 text-sm text-warning-foreground"
        >
          <TriangleAlert className="mt-0.5 size-4 shrink-0" aria-hidden="true" />
          {noticeText[fc.notice]()}
        </p>
      )}

      {days.length > 0 ? (
        <section aria-labelledby="fc-title" className="overflow-hidden sm:rounded-xl sm:border sm:bg-card">
          <div className="flex flex-wrap items-center justify-between gap-4 pb-3 sm:border-b sm:px-5 sm:py-4">
            <div>
              <h2 id="fc-title" className="m-0 font-semibold text-lg tracking-[-0.02em]">
                {m.forecast_title()}
              </h2>
              <p className="m-0 mt-1 text-muted-foreground text-[13px]">
                {m.forecast_note()}
                {taf && ` · ${m.taf_row()}: ${m.taf_legend()}`}
                {' · '}
                <Link to="/gear" className="text-foreground underline underline-offset-2">
                  {gear.length > 0 ? m.gear_edit() : m.gear_hint()}
                </Link>
              </p>
            </div>
            <div className="flex flex-wrap items-center gap-3">
              <ScaleLegend unit={unit} />
            </div>
          </div>
          <div className="flex flex-col gap-4 sm:p-3">
            <QuickView fc={fc} days={threeHourly} glances={glances} unit={unit} spot={fav?.name} />
            <ForecastDays fc={fc} days={days} unit={unit} now={now} measured={measured} taf={taf} gear={gear} />
          </div>
        </section>
      ) : (
        !fc.notice && (
          <p className="m-0 rounded-xl border bg-card px-5 py-8 text-center text-muted-foreground text-sm">
            {m.forecast_empty()}
          </p>
        )
      )}

      {history}

      {fc.series.length > 0 && (
        <div className="flex flex-wrap items-start gap-4">
          {/* How far the models agree, beside their comparison: below the forecast, not before it */}
          {fc.series.length > 1 && <AgreementTile fc={fc} />}
          <CompareChart
            fc={fc}
            selected={selected}
            onSelect={setPicked}
            observations={measured?.observations ?? []}
            source={measured?.source ?? null}
          />
          {ranking && rankingStation && (
            <RankingCard
              fc={fc}
              ranking={ranking}
              code={rankingStation.code}
              source={rankingStation.source}
              from={rankingStation.from}
              selected={selected}
              onSelect={setPicked}
            />
          )}
        </div>
      )}
    </div>
  );
}
