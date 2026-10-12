import { createFileRoute } from '@tanstack/react-router';
import { Page } from '@/components/page';
import { DATA_SOURCES, SOFTWARE, type Source } from '@/lib/sources';
import { useTitle } from '@/lib/use-title';
import { m } from '@/paraglide/messages';
import { Card, CardHeader, CardTitle } from '@/ui/card';

export const Route = createFileRoute('/sources')({
  component: SourcesPage,
});

const REPO = 'https://github.com/firsttris/windfenster';
const LINK = 'text-foreground underline decoration-line underline-offset-2 hover:decoration-foreground';

function Row({ source }: { source: Source }) {
  const licence = typeof source.licence.name === 'function' ? source.licence.name() : source.licence.name;
  return (
    <li className="grid gap-x-6 gap-y-1 border-line border-t py-3 first:border-t-0 first:pt-0 last:pb-0 md:grid-cols-[minmax(0,14rem)_minmax(0,1fr)_minmax(0,16rem)]">
      <a href={source.url} className={`${LINK} font-semibold`}>
        {source.name}
      </a>
      <p className="m-0 text-sm text-subtle">
        <span className="sr-only">{m.sources_use()}: </span>
        {source.use()}
      </p>
      <dl className="m-0 flex flex-col gap-0.5 text-sm">
        <div className="flex gap-1.5">
          <dt className="text-muted-foreground">{m.sources_licence()}</dt>
          <dd className="m-0">
            <a href={source.licence.url} className={LINK}>
              {licence}
            </a>
          </dd>
        </div>
        {source.credit && (
          <div className="flex gap-1.5">
            <dt className="shrink-0 text-muted-foreground">{m.sources_credit()}</dt>
            <dd className="m-0 font-mono text-xs leading-5">{source.credit}</dd>
          </div>
        )}
      </dl>
    </li>
  );
}

function Section({ title, sources }: { title: string; sources: readonly Source[] }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>{title}</CardTitle>
      </CardHeader>
      <ul className="m-0 flex list-none flex-col p-0">
        {sources.map((s) => (
          <Row key={s.id} source={s} />
        ))}
      </ul>
    </Card>
  );
}

function SourcesPage() {
  useTitle(m.title_sources());
  return (
    <Page>
      <div className="flex max-w-2xl flex-col gap-2">
        <h1 className="m-0 font-bold text-4xl tracking-[-0.05em]">{m.sources_title()}</h1>
        <p className="m-0 text-subtle leading-relaxed">{m.sources_lead()}</p>
      </div>
      <Section title={m.sources_data()} sources={DATA_SOURCES} />
      <Section title={m.sources_software()} sources={SOFTWARE} />
      <p className="m-0 text-muted-foreground text-sm">
        {m.sources_code()}{' '}
        <a href={REPO} className={LINK}>
          GitHub
        </a>
      </p>
    </Page>
  );
}
