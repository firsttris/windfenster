import { useQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { useState } from 'react';
import type { WindUnit } from '@/bindings/WindUnit';
import { healthQuery, modelsQuery } from '@/lib/api';
import { describeError, intlLocale } from '@/lib/i18n';
import { formatWind, UNIT_LABEL, WIND_UNITS } from '@/lib/units';
import { cn } from '@/lib/utils';
import { m } from '@/paraglide/messages';
import { Badge } from '@/ui/badge';
import { Card, CardHeader, CardMeta, CardTitle } from '@/ui/card';
import { Segmented } from '@/ui/segmented';
import { WindCell } from '@/ui/wind-cell';
import { WIND_SCALE } from '@/ui/wind-scale';

export const Route = createFileRoute('/')({
  loader: ({ context }) => {
    // Start both requests with the route; the page renders while they run.
    void context.queryClient.prefetchQuery(healthQuery);
    void context.queryClient.prefetchQuery(modelsQuery);
  },
  component: Home,
});

// Phase 0 shows that the stack runs end to end; station search and pages come in Phase 1.
function Home() {
  const [unit, setUnit] = useState<WindUnit>('kn');
  return (
    <main className="mx-auto flex w-full max-w-[1360px] flex-col gap-5 px-4 pt-8 pb-12 sm:px-6">
      <section className="flex flex-col gap-2.5">
        <div className="flex flex-wrap items-center gap-2">
          <Badge>{m.home_badgeOpenSource()}</Badge>
          <Badge>{m.home_badgeSelfHosting()}</Badge>
          <Badge variant="secondary">{m.home_badgePhase()}</Badge>
        </div>
        <h1 className="m-0 text-5xl leading-none font-bold tracking-[-0.05em] sm:text-6xl">Windfenster</h1>
        <p className="m-0 max-w-2xl text-[15px] leading-relaxed text-subtle">{m.home_lead()}</p>
      </section>

      <section className="grid items-start gap-4 [grid-template-columns:repeat(auto-fit,minmax(300px,1fr))]">
        <ServerCard />
        <ScaleCard unit={unit} onUnit={setUnit} />
        <ModelsCard />
      </section>
    </main>
  );
}

function ServerCard() {
  const health = useQuery(healthQuery);
  const up = health.isSuccess;
  const rows = health.data
    ? [
        [m.status_version(), health.data.version],
        [m.status_schema(), String(health.data.schemaVersion)],
        [m.status_stations(), String(health.data.stations)],
        [m.status_models(), String(health.data.models)],
        [m.status_runs(), String(health.data.modelRuns)],
      ]
    : [];
  return (
    <Card aria-busy={health.isPending}>
      <CardHeader>
        <CardTitle>
          <span
            className={cn(
              'size-2 rounded-full',
              up ? 'bg-success shadow-[0_0_0_4px_var(--success-ring)]' : health.isError ? 'bg-destructive' : 'bg-dim',
            )}
          />
          {m.status_title()}
        </CardTitle>
        <CardMeta>/api/health</CardMeta>
      </CardHeader>
      <div className="text-[42px] leading-none font-bold tracking-[-0.045em]">
        {up ? m.status_up() : health.isError ? m.status_down() : m.status_loading()}
      </div>
      {health.isError && <p className="m-0 text-sm text-muted-foreground">{describeError(health.error)}</p>}
      {rows.length > 0 && (
        <dl className="m-0 grid grid-cols-[1fr_auto] gap-x-3 gap-y-1.5 border-t pt-3.5 text-sm">
          {rows.map(([k, v]) => (
            <div key={k} className="contents">
              <dt className="text-muted-foreground">{k}</dt>
              <dd className="m-0 text-right font-mono font-semibold">{v}</dd>
            </div>
          ))}
        </dl>
      )}
    </Card>
  );
}

// One value from the middle of each band, as the forecast table will show it.
const SAMPLES = [3, 7, 11, 15, 19, 23, 27, 33];

function ScaleCard({ unit, onUnit }: { unit: WindUnit; onUnit: (u: WindUnit) => void }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>{m.scale_title()}</CardTitle>
        <CardMeta>{m.scale_edges()}</CardMeta>
      </CardHeader>
      <div className="flex overflow-hidden rounded-[7px]" role="img" aria-label={m.scale_label()}>
        {WIND_SCALE.map((b, i) => (
          <span
            key={b.from}
            className="flex-1 py-1.5 text-center font-mono text-[11px] font-semibold"
            style={{ background: b.bg, color: b.fg }}
          >
            {i === WIND_SCALE.length - 1 ? `${b.from}+` : b.from}
          </span>
        ))}
      </div>
      <Segmented
        label={m.scale_unit()}
        value={unit}
        options={WIND_UNITS.map((u) => ({ value: u, label: UNIT_LABEL[u] }))}
        onChange={onUnit}
        className="self-start"
      />
      <div className="grid grid-cols-[repeat(4,52px)] gap-1">
        {SAMPLES.map((kn) => (
          <WindCell key={kn} kn={kn} unit={unit} />
        ))}
      </div>
      <p className="m-0 text-xs leading-snug text-dim">
        Ein Beispielwert je Stufe ({SAMPLES.map((kn) => formatWind(kn, 'kn')).join(', ')} kn). Der Server rechnet in
        Knoten, umgerechnet wird erst in der Anzeige.
      </p>
    </Card>
  );
}

function ModelsCard() {
  const models = useQuery(modelsQuery);
  return (
    <Card>
      <CardHeader>
        <CardTitle>{m.models_title()}</CardTitle>
        <CardMeta>/api/models</CardMeta>
      </CardHeader>
      {models.data && models.data.length > 0 ? (
        <ul className="m-0 flex list-none flex-col gap-2 p-0">
          {models.data.map((model, i) => (
            <li key={model.id} className="flex items-center gap-2.5 text-sm">
              <span className="size-2 shrink-0 rounded-full" style={{ background: `var(--chart-${(i % 5) + 1})` }} />
              <span className="font-semibold">{model.name}</span>
              <span className="font-mono text-xs text-muted-foreground">
                {model.resolutionKm.toLocaleString(intlLocale())} km ·{' '}
                {model.class === 'super' ? m.class_super() : m.class_global()}
              </span>
            </li>
          ))}
        </ul>
      ) : (
        <p className="m-0 text-sm leading-relaxed text-subtle">
          {models.isError
            ? m.models_failed({ error: describeError(models.error) })
            : models.isPending
              ? m.models_loading()
              : m.models_none()}
        </p>
      )}
      <div className="flex flex-wrap gap-1.5 border-t pt-3.5">
        {[m.models_superHint(), m.models_globalHint()].map((t) => (
          <Badge key={t} variant="secondary">
            {t}
          </Badge>
        ))}
      </div>
    </Card>
  );
}
