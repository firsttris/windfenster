import { Link } from '@tanstack/react-router';
import type { ReactNode } from 'react';
import { isApiError } from '@/lib/api';
import { describeError } from '@/lib/i18n';
import { m } from '@/paraglide/messages';

export function Page({ children }: { children: ReactNode }) {
  return <main className="mx-auto flex w-full max-w-[1360px] flex-col gap-5 px-4 pt-8 pb-12 sm:px-6">{children}</main>;
}

/** Placeholder blocks in the page's shape while it loads. */
export function Loading() {
  return (
    <Page>
      <div role="status" aria-busy="true" className="flex animate-pulse flex-col gap-4">
        <span className="sr-only">{m.page_loading()}</span>
        <div className="h-4 w-48 rounded bg-muted" />
        <div className="h-14 w-full max-w-lg rounded bg-muted" />
        <div className="h-20 rounded-xl bg-muted" />
        <div className="h-[420px] rounded-xl bg-muted" />
      </div>
    </Page>
  );
}

export function Failed({ error }: { error: unknown }) {
  const notFound = isApiError(error) && error.status === 404;
  return (
    <Page>
      <h1 className="m-0 font-bold text-4xl tracking-[-0.05em]">{notFound ? m.notFound_title() : m.page_failed()}</h1>
      <p className="m-0 text-muted-foreground">
        {notFound ? m.station_notFound() : describeError(error)}{' '}
        <Link to="/" className="text-subtle hover:text-foreground">
          {m.notFound_home()}
        </Link>
      </p>
    </Page>
  );
}
