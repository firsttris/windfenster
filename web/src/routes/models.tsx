import { useQuery } from '@tanstack/react-query';
import { createFileRoute, Link, useLocation } from '@tanstack/react-router';
import { useEffect } from 'react';
import type { Model } from '@/bindings/Model';
import { Loading, Page } from '@/components/page';
import { modelsQuery } from '@/lib/api';
import { MIX } from '@/lib/forecast';
import { formatNumber } from '@/lib/format';
import { modelColor } from '@/lib/model-color';
import { inOrder, MODEL_GUIDE, REGIONS, running } from '@/lib/model-guide';
import { useTitle } from '@/lib/use-title';
import { cn } from '@/lib/utils';
import { m } from '@/paraglide/messages';
import { Card, CardHeader, CardTitle } from '@/ui/card';

export const Route = createFileRoute('/models')({
  component: ModelsPage,
});

/** What each model is good for and how Windfenster chooses; the model chips link here, to the
 * chosen model's card (`#icon_d2`). */
function ModelsPage() {
  useTitle(m.title_guide());
  const models = useQuery(modelsQuery);
  const hash = useLocation({ select: (l) => l.hash });
  // The cards come with the model list, after the router tried to scroll to the hash
  useEffect(() => {
    if (models.data && hash) document.getElementById(hash)?.scrollIntoView({ block: 'start' });
  }, [models.data, hash]);
  if (!models.data) return <Loading />;
  const list = inOrder(models.data);
  const all = { models: list };
  return (
    <Page>
      <div className="flex max-w-2xl flex-col gap-2">
        <h1 className="m-0 font-bold text-4xl tracking-[-0.05em]">{m.guide_title()}</h1>
        <p className="m-0 text-subtle leading-relaxed">{m.guide_lead()}</p>
      </div>
      <Card>
        <CardHeader>
          <CardTitle>{m.guide_which()}</CardTitle>
        </CardHeader>
        <p className="m-0 max-w-2xl text-sm text-subtle leading-relaxed">{m.guide_whichLead()}</p>
        <table className="w-full border-collapse text-left">
          <thead className="hidden md:table-header-group">
            <tr className="text-muted-foreground text-xs">
              <th className="w-[38%] pb-2 font-normal">{m.guide_region()}</th>
              <th className="pb-2 font-normal">{m.guide_soon()}</th>
              <th className="pb-2 font-normal">{m.guide_later()}</th>
            </tr>
          </thead>
          <tbody>
            {REGIONS.map((r) => (
              <tr key={r.name()} className="grid gap-y-1.5 border-line border-t py-3 md:table-row md:py-0">
                <th scope="row" className="pr-6 text-left font-semibold text-sm md:py-3">
                  {r.name()}
                </th>
                <Pills label={m.guide_soon()} models={running(r.soon, list)} all={all} />
                <Pills label={m.guide_later()} models={running(r.later, list)} all={all} />
              </tr>
            ))}
          </tbody>
        </table>
      </Card>
      <Card>
        <CardHeader>
          <CardTitle>{m.guide_choose()}</CardTitle>
        </CardHeader>
        <div className="grid gap-5 md:grid-cols-2">
          <section className="flex flex-col gap-1.5">
            <h3 className="m-0 font-semibold text-base">{m.guide_auto()}</h3>
            <p className="m-0 text-sm text-subtle leading-relaxed">{m.guide_autoText()}</p>
          </section>
          <section className="flex flex-col gap-1.5">
            <h3 className="m-0 flex items-center gap-2 font-semibold text-base">
              <span aria-hidden="true" className="size-2.5 rounded-full" style={{ background: modelColor(all, MIX) }} />
              {m.mix_name()}
            </h3>
            <p className="m-0 text-sm text-subtle leading-relaxed">{m.guide_mixText()}</p>
          </section>
        </div>
        <p className="m-0 border-line border-t pt-3.5 text-muted-foreground text-sm leading-relaxed">
          {m.guide_classes()}
        </p>
      </Card>
      <h2 className="m-0 mt-3 font-bold text-2xl tracking-[-0.03em]">{m.guide_models()}</h2>
      <div className="grid gap-4 lg:grid-cols-2">
        {list.map((x) => (
          <ModelCard key={x.id} model={x} all={all} target={hash === x.id} />
        ))}
      </div>
    </Page>
  );
}

/** The models to look at, each a link to its card; a dash where this server runs none of them */
function Pills({ label, models, all }: { label: string; models: Model[]; all: { models: Model[] } }) {
  return (
    <td className="md:py-3 md:pr-4">
      <span className="flex flex-wrap items-center gap-1.5">
        <span className="w-full text-muted-foreground text-xs md:hidden">{label}</span>
        {models.length === 0
          ? '–'
          : models.map((x) => (
              <Link
                key={x.id}
                to="/models"
                hash={x.id}
                // Scrolls itself, also when the hash already names this model (the router then
                // does nothing); the router's reset to the top would undo it
                resetScroll={false}
                onClick={() => document.getElementById(x.id)?.scrollIntoView({ block: 'start' })}
                className="inline-flex h-7 items-center gap-1.5 rounded-md border bg-background px-2.5 font-semibold text-[13px] outline-none hover:border-border-strong focus-visible:ring-2 focus-visible:ring-ring/60"
              >
                <span
                  aria-hidden="true"
                  className="size-2 rounded-full"
                  style={{ background: modelColor(all, x.id) }}
                />
                {x.name}
              </Link>
            ))}
      </span>
    </td>
  );
}

/** "2,2 km · 2 Tage · 8 Läufe am Tag" */
function facts(x: Model): string {
  const km = `${formatNumber(x.resolutionKm, Number.isInteger(x.resolutionKm) ? 0 : 1)} km`;
  return `${km} · ${m.models_days({ days: Math.round(x.maxLeadH / 24) })} · ${m.guide_runs({ count: x.runsPerDay })}`;
}

function ModelCard({ model, all, target }: { model: Model; all: { models: Model[] }; target: boolean }) {
  const guide = MODEL_GUIDE[model.id];
  const area = guide?.area?.() ?? (model.domain ? null : m.guide_world());
  return (
    <Card
      id={model.id}
      aria-labelledby={`${model.id}-name`}
      className={cn('scroll-mt-24 transition-shadow', target && 'border-primary ring-2 ring-primary/30')}
    >
      <div className="flex flex-wrap items-baseline justify-between gap-x-3 gap-y-1">
        <h3 id={`${model.id}-name`} className="m-0 flex items-center gap-2.5 font-bold text-xl tracking-[-0.02em]">
          <span aria-hidden="true" className="size-3 rounded-full" style={{ background: modelColor(all, model.id) }} />
          {model.name}
        </h3>
        <span className="text-muted-foreground text-sm">
          {model.provider} · {model.class === 'super' ? m.class_super() : m.class_global()}
        </span>
      </div>
      <p className="m-0 font-mono text-subtle text-xs">{facts(model)}</p>
      {guide && (
        <dl className="m-0 grid gap-x-4 gap-y-2 text-sm leading-relaxed sm:grid-cols-[6.5rem_minmax(0,1fr)]">
          {area && (
            <>
              <dt className="text-muted-foreground">{m.guide_area()}</dt>
              <dd className="m-0">{area}</dd>
            </>
          )}
          <dt className="text-muted-foreground">{m.guide_good()}</dt>
          <dd className="m-0">{guide.good()}</dd>
          <dt className="text-muted-foreground">{m.guide_watch()}</dt>
          <dd className="m-0 text-subtle">{guide.watch()}</dd>
        </dl>
      )}
    </Card>
  );
}
