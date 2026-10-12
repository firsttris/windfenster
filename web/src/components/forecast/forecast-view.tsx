import { Link } from '@tanstack/react-router';
import { TriangleAlert } from 'lucide-react';
import { type ReactNode, useMemo, useState } from 'react';
import type { Forecast } from '@/bindings/Forecast';
import type { Ranking } from '@/bindings/Ranking';
import type { Taf } from '@/bindings/Taf';
import { AgreementTile } from '@/components/verification/agreement-tile';
import { CompareChart } from '@/components/verification/compare-chart';
import { RankingCard } from '@/components/verification/ranking-card';
import { autoModel, buildDays, initialModel, MIX, type Mode, modeOf } from '@/lib/forecast';
import { ago, runHour } from '@/lib/format';
import { noticeText } from '@/lib/i18n';
import type { Measured } from '@/lib/observations';
import { useUnit } from '@/lib/prefs';
import { UNIT_LABEL, WIND_UNITS } from '@/lib/units';
import { useGear } from '@/lib/use-gear';
import { useMedia } from '@/lib/use-media';
import { useNow } from '@/lib/use-now';
import { m } from '@/paraglide/messages';
import { Segmented } from '@/ui/segmented';
import { ForecastList } from './forecast-list';
import { ForecastTable } from './forecast-table';
import { ModelChips } from './model-chips';
import { modelName, ScaleLegend } from './shared';

interface Props {
  fc: Forecast;
  /** Cards between the models and the table, e.g. the live measurement. */
  tiles?: ReactNode;
  /** Measured values for the row under the forecast (desktop). */
  measured?: Measured;
  /** The station's TAF, for its row (desktop). */
  taf?: Taf | null;
  /** "Who hits here?": the station's, or for a point the nearest station's within 5 km */
  ranking?: Ranking | null;
  /** The station's measurements over the day, below the table */
  history?: ReactNode;
  /** The station the ranking belongs to: its code, and for points how far away it is */
  rankingStation?: { code: string; source: string; from?: string };
}

/** Everything below a place's title: mode and unit, the models, tiles, the ten days. */
export function ForecastView({ fc, tiles, measured, taf, ranking, rankingStation, history }: Props) {
  const [unit, setUnit] = useUnit();
  const [picked, setPicked] = useState<string | null>(null);
  const superModel = autoModel(fc, ranking, 'super');
  const globalModel = autoModel(fc, ranking, 'global');
  const selected = picked && fc.series.some((s) => s.model === picked) ? picked : initialModel(fc, ranking);
  const now = useNow();
  const { gear } = useGear();
  const days = useMemo(
    () => (selected ? buildDays(fc, selected, now, globalModel) : []),
    [fc, selected, now, globalModel],
  );
  const desktop = useMedia('(min-width: 768px)', true);
  const series = fc.series.find((s) => s.model === selected);
  const mode = selected ? modeOf(fc, selected) : null;
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
      <div className="flex flex-wrap items-center justify-between gap-3">
        <Segmented<Mode>
          className="w-full sm:w-auto"
          stretch={!desktop}
          label={m.forecast_mode()}
          value={mode}
          options={[
            { value: 'super', label: m.class_super(), disabled: !superModel },
            { value: 'global', label: m.class_global(), disabled: !globalModel },
          ]}
          onChange={(v) => setPicked(v === 'super' ? superModel : globalModel)}
        />
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

      <div className="grid items-start gap-4 md:grid-cols-2 xl:grid-cols-3">
        {tiles}
        {fc.series.length > 1 && <AgreementTile fc={fc} />}
      </div>

      {days.length > 0 ? (
        <section aria-labelledby="fc-title" className="overflow-hidden sm:rounded-xl sm:border sm:bg-card">
          <div className="flex flex-wrap items-center justify-between gap-4 pb-3 sm:border-b sm:px-5 sm:py-4">
            <div>
              <h2 id="fc-title" className="m-0 font-semibold text-lg tracking-[-0.02em]">
                {m.forecast_title()}
              </h2>
              <p className="m-0 mt-1 text-muted-foreground text-[13px]">
                {m.forecast_note()}
                {taf && desktop && ` · ${m.taf_row()}: ${m.taf_legend()}`}
                {' · '}
                <Link to="/gear" className="text-foreground underline underline-offset-2">
                  {gear.length > 0 ? m.gear_edit() : m.gear_hint()}
                </Link>
              </p>
            </div>
            <ScaleLegend unit={unit} />
          </div>
          <div className={desktop ? 'pb-3' : ''}>
            {desktop ? (
              <ForecastTable fc={fc} days={days} unit={unit} now={now} measured={measured} taf={taf} gear={gear} />
            ) : (
              <ForecastList fc={fc} days={days} unit={unit} gear={gear} />
            )}
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
