import { createFileRoute, Link } from '@tanstack/react-router';
import { ArrowRight } from 'lucide-react';
import { Page } from '@/components/page';
import { SearchButton } from '@/components/search';
import { useTitle } from '@/lib/use-title';
import { m } from '@/paraglide/messages';
import { WIND_SCALE } from '@/ui/wind-scale';

export const Route = createFileRoute('/')({
  component: Home,
});

// The plan's reference stations: on the Rhine near Iffezheim, and Rhodes.
const EXAMPLES = [
  { slug: 'edsb', code: 'EDSB', name: m.home_exampleEdsb, text: m.home_exampleEdsbText },
  { slug: 'lgrp', code: 'LGRP', name: m.home_exampleLgrp, text: m.home_exampleLgrpText },
] as const;

function Home() {
  useTitle(m.title_home());
  return (
    <Page>
      <section className="flex flex-col gap-5 py-6 sm:py-12">
        <div className="flex overflow-hidden rounded-full" aria-hidden="true">
          {WIND_SCALE.map((b) => (
            <span key={b.from} className="h-1.5 flex-1" style={{ background: b.bg }} />
          ))}
        </div>
        <h1 className="m-0 font-bold text-5xl leading-none tracking-[-0.05em] sm:text-7xl">{m.home_title()}</h1>
        <p className="m-0 max-w-2xl text-lg text-subtle leading-relaxed">{m.home_lead()}</p>
        <SearchButton large className="max-w-2xl" />
      </section>
      <section aria-labelledby="examples" className="flex flex-col gap-3">
        <h2 id="examples" className="m-0 font-medium text-[13px] text-muted-foreground">
          {m.home_examples()}
        </h2>
        <div className="grid gap-4 [grid-template-columns:repeat(auto-fit,minmax(280px,1fr))]">
          {EXAMPLES.map((e) => (
            <Link
              key={e.slug}
              to="/station/$slug"
              params={{ slug: e.slug }}
              className="group flex flex-col gap-2 rounded-xl border bg-card p-5 text-foreground no-underline transition-colors hover:border-border-strong"
            >
              <span className="flex items-baseline justify-between gap-2">
                <span className="font-bold text-2xl tracking-[-0.04em]">{e.name()}</span>
                <span className="font-mono text-muted-foreground text-sm">{e.code}</span>
              </span>
              <span className="text-muted-foreground text-sm">{e.text()}</span>
              <ArrowRight
                className="size-4 text-muted-foreground transition-transform group-hover:translate-x-0.5"
                aria-hidden="true"
              />
            </Link>
          ))}
        </div>
      </section>
    </Page>
  );
}
