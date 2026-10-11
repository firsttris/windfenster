import { TriangleAlert } from 'lucide-react';
import { type ReactNode, useMemo, useState } from 'react';
import type { Forecast } from '@/bindings/Forecast';
import { buildDays, defaultModel, type Mode, modeOf } from '@/lib/forecast';
import { ago, runHour } from '@/lib/format';
import { noticeText } from '@/lib/i18n';
import type { Measured } from '@/lib/observations';
import { useUnit } from '@/lib/prefs';
import { UNIT_LABEL, WIND_UNITS } from '@/lib/units';
import { useMedia } from '@/lib/use-media';
import { useNow } from '@/lib/use-now';
import { m } from '@/paraglide/messages';
import { Segmented } from '@/ui/segmented';
import { ForecastList } from './forecast-list';
import { ForecastTable } from './forecast-table';
import { ModelChips } from './model-chips';
import { ScaleLegend } from './shared';

interface Props {
  fc: Forecast;
  /** Cards between the models and the table, e.g. the live measurement. */
  tiles?: ReactNode;
  /** Measured values for the row under the forecast (desktop). */
  measured?: Measured;
}

/** Everything below a place's title: mode and unit, the models, tiles, the ten days. */
export function ForecastView({ fc, tiles, measured }: Props) {
  const [unit, setUnit] = useUnit();
  const [picked, setPicked] = useState<string | null>(null);
  const superModel = defaultModel(fc, 'super');
  const globalModel = defaultModel(fc, 'global');
  const selected = picked && fc.series.some((s) => s.model === picked) ? picked : (superModel ?? globalModel);
  const now = useNow();
  const days = useMemo(() => (selected ? buildDays(fc, selected, now) : []), [fc, selected, now]);
  const desktop = useMedia('(min-width: 768px)', true);
  const series = fc.series.find((s) => s.model === selected);
  const mode: Mode = selected ? modeOf(fc, selected) : 'global';

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

      {fc.models.length > 0 && <ModelChips fc={fc} selected={selected} onSelect={setPicked} />}

      {fc.notice && (
        <p
          role="status"
          className="m-0 flex items-start gap-2.5 rounded-xl border border-warning-border bg-warning-surface px-4 py-3 text-sm text-warning-foreground"
        >
          <TriangleAlert className="mt-0.5 size-4 shrink-0" aria-hidden="true" />
          {noticeText[fc.notice]()}
        </p>
      )}

      {tiles && <div className="grid items-start gap-4 md:grid-cols-2 xl:grid-cols-3">{tiles}</div>}

      {days.length > 0 ? (
        <section aria-labelledby="fc-title" className="overflow-hidden sm:rounded-xl sm:border sm:bg-card">
          <div className="flex flex-wrap items-center justify-between gap-4 pb-3 sm:border-b sm:px-5 sm:py-4">
            <div>
              <h2 id="fc-title" className="m-0 font-semibold text-lg tracking-[-0.02em]">
                {m.forecast_title()}
              </h2>
              <p className="m-0 mt-1 text-muted-foreground text-[13px]">{m.forecast_note()}</p>
            </div>
            <ScaleLegend unit={unit} />
          </div>
          <div className={desktop ? 'pb-3' : ''}>
            {desktop ? (
              <ForecastTable fc={fc} days={days} unit={unit} now={now} measured={measured} />
            ) : (
              <ForecastList fc={fc} days={days} unit={unit} />
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
    </div>
  );
}
