/** The admin page's forecasts: how often they update, the Open-Meteo budget with two weeks of
 * use, the models in use with their newest run, the settings and the places kept current. */
import { useQuery } from '@tanstack/react-query';
import { Info } from 'lucide-react';
import type { AdminForecasts } from '@/bindings/AdminForecasts';
import type { ModelRunState } from '@/bindings/ModelRunState';
import { Failed } from '@/components/page';
import { percent } from '@/lib/admin';
import { adminForecastsQuery } from '@/lib/admin-api';
import { ago, coordinates, formatNumber, runHour } from '@/lib/format';
import { m } from '@/paraglide/messages';
import { Card } from '@/ui/card';
import { Chip, ROW, TabHead, Table } from './parts';
import { NumberField, SettingRow, Toggle, useSettings } from './setting';

export function ForecastsTab() {
  const f = useQuery(adminForecastsQuery);
  const settings = useSettings();
  if (f.isError) return <Failed error={f.error} />;
  if (!f.data) return <div aria-busy="true" className="h-96 animate-pulse rounded-xl bg-muted" />;
  return (
    <>
      <TabHead title={m.admin_tabForecasts()} lead={m.admin_forecastsLead()} />
      <div className="flex items-start gap-3.5 rounded-xl border bg-card px-4 py-3.5 text-[13px] text-subtle leading-relaxed">
        <Info aria-hidden="true" className="mt-0.5 size-[18px] shrink-0 text-muted-foreground" />
        <span>
          <strong className="text-foreground">{m.admin_howOften()}</strong>{' '}
          {m.admin_howOftenText({ days: settings.data?.settings.hotDays ?? 7 })}
        </span>
      </div>
      <BudgetCard data={f.data} />
      <ModelsCard runs={f.data.runs} hot={f.data.hotTotal} />
      <div className="grid grid-cols-[repeat(auto-fit,minmax(340px,1fr))] items-start gap-5">
        <SettingsCard />
        <HotCard data={f.data} />
      </div>
    </>
  );
}

function BudgetCard({ data }: { data: AdminForecasts }) {
  const b = data.budget;
  const used = b.refresh + b.opened;
  const limit = b.limit ?? Math.max(...data.days.map((d) => d.refresh + d.opened), 1);
  const top = Math.max(limit, ...data.days.map((d) => d.refresh + d.opened), 1);
  const peak = data.days.reduce((a, d) => Math.max(a, d.refresh + d.opened), 0);
  return (
    <Card className="gap-4">
      <div className="flex flex-wrap items-end justify-between gap-3">
        <div className="flex flex-col gap-1">
          <h2 className="m-0 font-semibold text-base">{m.admin_budget()}</h2>
          <span className="text-muted-foreground text-xs">
            {data.withKey ? m.admin_budgetKey() : m.admin_budgetFree()} · {m.admin_budgetHint()}
          </span>
        </div>
        <div className="flex items-baseline gap-1.5">
          <span className="font-bold text-[32px] tracking-[-0.04em]">{formatNumber(used)}</span>
          {b.limit !== null && (
            <span className="font-mono text-[13px] text-muted-foreground">
              {m.admin_kpiBudgetOf({ limit: formatNumber(b.limit) })}
            </span>
          )}
        </div>
      </div>
      {b.limit !== null && (
        <div className="relative flex h-3 overflow-hidden rounded-full bg-line">
          <div className="h-full bg-chart-1" style={{ width: `${percent(b.refresh, b.limit)}%` }} />
          <div className="h-full bg-chart-3" style={{ width: `${percent(b.opened, b.limit)}%` }} />
          {b.openedLimit !== null && (
            <div
              className="absolute inset-y-0 w-0.5 bg-foreground"
              style={{ left: `${percent(b.openedLimit, b.limit)}%` }}
            />
          )}
        </div>
      )}
      <div className="flex flex-wrap gap-4 text-muted-foreground text-xs">
        <span className="flex items-center gap-1.5">
          <span className="size-2.5 rounded-[3px] bg-chart-1" />
          {m.admin_budgetRefresh({ n: formatNumber(b.refresh) })}
        </span>
        <span className="flex items-center gap-1.5">
          <span className="size-2.5 rounded-[3px] bg-chart-3" />
          {m.admin_budgetOpened({ n: formatNumber(b.opened) })}
        </span>
        {b.openedLimit !== null && (
          <span className="flex items-center gap-1.5">
            <span className="h-3 w-0.5 bg-foreground" />
            {m.admin_budgetLine()} · {formatNumber(b.openedLimit)}
          </span>
        )}
      </div>
      <figure className="m-0 flex flex-col gap-1.5 border-line border-t pt-3">
        <div
          role="img"
          aria-label={`${m.admin_budgetChart()}: ${formatNumber(peak)}`}
          className="flex h-[90px] items-end gap-1.5"
        >
          {data.days.map((d) => (
            <div
              key={d.day}
              className="flex h-full flex-1 flex-col justify-end"
              title={formatNumber(d.refresh + d.opened)}
            >
              <div className="rounded-t-[3px] bg-chart-3" style={{ height: `${(d.opened / top) * 100}%` }} />
              <div className="bg-chart-1" style={{ height: `${(d.refresh / top) * 100}%` }} />
            </div>
          ))}
        </div>
        <figcaption className="flex justify-between font-mono text-[11px] text-muted-foreground">
          <span>{m.admin_budgetChart()}</span>
          <span>{formatNumber(peak)}</span>
        </figcaption>
      </figure>
    </Card>
  );
}

function runState(r: ModelRunState): { tone: 'ok' | 'warn' | 'running' | 'off'; label: string } {
  if (r.runAt === null) return { tone: 'off', label: m.admin_noRuns() };
  if (r.ingestedAt === null) return { tone: 'running', label: m.admin_runFetching() };
  if (r.estimated) return { tone: 'warn', label: m.admin_runEstimated() };
  return { tone: 'ok', label: m.admin_runDone() };
}

function ModelsCard({ runs, hot }: { runs: ModelRunState[]; hot: number }) {
  const settings = useSettings();
  const s = settings.data;
  const inUse = (id: string) => s?.settings.models?.includes(id) ?? true;
  const toggle = (id: string, on: boolean) => {
    if (!s) return;
    const all = s.models.map(([mid]) => mid);
    const now = all.filter((mid) => (mid === id ? on : inUse(mid)));
    settings.set('models', now.length === all.length ? null : now);
  };
  const name = (id: string) => s?.models.find(([mid]) => mid === id)?.[1] ?? id;
  return (
    <Card className="gap-3.5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h2 className="m-0 font-semibold text-base">{m.admin_models()}</h2>
        <span className="text-muted-foreground text-xs">{m.admin_modelsHint({ count: hot })}</span>
      </div>
      <Table
        cols="1.8fr 1.4fr 1.2fr 1.1fr 0.6fr"
        head={[
          m.admin_colModel(),
          m.admin_colRun(),
          m.admin_colPlaces(),
          m.admin_colState(),
          <span key="o" className="block text-right">
            {m.admin_colOn()}
          </span>,
        ]}
      >
        {runs.map((r) => {
          const st = runState(r);
          return (
            <div key={r.model} className={ROW}>
              <span className="flex flex-col">
                <span className="font-semibold">{name(r.model)}</span>
                <span className="font-mono text-[11px] text-muted-foreground">{r.model}</span>
              </span>
              <span className="font-mono text-subtle text-xs">
                {r.runAt === null ? '–' : `${runHour(r.runAt)} · ${ago(r.runAt)}`}
              </span>
              <span className="font-mono text-subtle text-xs">{formatNumber(r.places)}</span>
              <span>
                <Chip tone={st.tone}>{st.label}</Chip>
              </span>
              <span className="flex justify-end">
                <Toggle
                  label={name(r.model)}
                  on={inUse(r.model)}
                  disabled={settings.pending || !!settings.locked('models') || !s}
                  onChange={(on) => toggle(r.model, on)}
                />
              </span>
            </div>
          );
        })}
      </Table>
      {settings.error && <p className="m-0 text-destructive text-sm">{settings.error}</p>}
    </Card>
  );
}

function SettingsCard() {
  const settings = useSettings();
  const s = settings.data?.settings;
  if (!s) return <Card aria-busy="true" className="h-72 animate-pulse" />;
  const busy = settings.pending;
  const field = (
    key: 'hotDays' | 'pointsDays' | 'placesPerHour' | 'placesPerDay' | 'searchesPerHour',
    label: string,
    hint: string,
    max: number,
  ) => (
    <SettingRow label={label} hint={hint} lockedBy={settings.locked(key)}>
      <NumberField
        label={label}
        value={s[key]}
        min={1}
        max={max}
        disabled={busy || !!settings.locked(key)}
        onSave={(n) => settings.set(key, n)}
      />
    </SettingRow>
  );
  return (
    <Card className="gap-3">
      <h2 className="m-0 font-semibold text-base">{m.admin_fcSettings()}</h2>
      {field('hotDays', m.admin_hotDays(), m.admin_hotDaysHint(), 60)}
      {field('pointsDays', m.admin_pointsDays(), m.admin_pointsDaysHint(), 60)}
      <SettingRow
        label={m.admin_budgetSetting()}
        hint={m.admin_budgetSettingHint()}
        lockedBy={settings.locked('openMeteoBudget')}
      >
        <BudgetField
          value={s.openMeteoBudget}
          disabled={busy || !!settings.locked('openMeteoBudget')}
          onSave={(v) => settings.set('openMeteoBudget', v)}
        />
      </SettingRow>
      {field('placesPerHour', m.admin_placesHour(), m.admin_placesHint(), 100_000)}
      {field('placesPerDay', m.admin_placesDay(), m.admin_placesHint(), 1_000_000)}
      {field('searchesPerHour', m.admin_searchesHour(), m.admin_searchesHint(), 100_000)}
      {settings.error && <p className="m-0 text-destructive text-sm">{settings.error}</p>}
    </Card>
  );
}

/** The daily limit: a number, or empty for the default. */
function BudgetField({
  value,
  disabled,
  onSave,
}: {
  value: number | null;
  disabled: boolean;
  onSave: (v: number | null) => void;
}) {
  return (
    <input
      aria-label={m.admin_budgetSetting()}
      inputMode="numeric"
      defaultValue={value ?? ''}
      placeholder="9000"
      disabled={disabled}
      onBlur={(e) => {
        const t = e.target.value.trim();
        const n = t === '' ? null : Number(t);
        if (n === value) return;
        if (n === null || (Number.isInteger(n) && n > 0)) onSave(n);
        else e.target.value = value === null ? '' : String(value);
      }}
      className="h-9 w-24 rounded-[9px] border bg-background px-3 text-right font-mono text-[13px] outline-none focus-visible:ring-2 focus-visible:ring-ring/60 disabled:opacity-60"
    />
  );
}

function HotCard({ data }: { data: AdminForecasts }) {
  return (
    <Card className="gap-3">
      <div className="flex justify-between gap-3">
        <h2 className="m-0 font-semibold text-base">{m.admin_hot()}</h2>
        <span className="font-mono text-muted-foreground text-xs">{formatNumber(data.hotTotal)}</span>
      </div>
      {data.hot.map((h) => (
        <div
          key={`${h.lat}_${h.lon}`}
          className="flex items-center justify-between gap-3 border-line border-b pb-2 text-[13px]"
        >
          <span className="flex min-w-0 flex-col">
            <span className="truncate font-medium">{h.name ?? coordinates(h.lat, h.lon)}</span>
            {h.name && <span className="font-mono text-[11px] text-muted-foreground">{coordinates(h.lat, h.lon)}</span>}
          </span>
          <span className="shrink-0 text-right text-muted-foreground text-xs">
            {h.tracked ? m.admin_hotTracked() : h.requestedAt ? m.admin_hotOpened({ ago: ago(h.requestedAt) }) : ''}
          </span>
        </div>
      ))}
      {data.hotTotal > data.hot.length && (
        <span className="text-muted-foreground text-xs">
          {m.admin_hotMore({ count: data.hotTotal - data.hot.length })}
        </span>
      )}
    </Card>
  );
}
