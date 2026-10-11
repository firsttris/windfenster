import type { QueryClient } from '@tanstack/react-query';
import { createRootRouteWithContext, Link, Outlet } from '@tanstack/react-router';
import { m } from '@/paraglide/messages';
import { LanguageSwitch } from '@/ui/language-switch';
import { Logo } from '@/ui/logo';

export const Route = createRootRouteWithContext<{ queryClient: QueryClient }>()({
  component: RootLayout,
  notFoundComponent: NotFound,
});

const REPO = 'https://github.com/firsttris/windfenster';

function RootLayout() {
  return (
    <div className="flex min-h-dvh flex-col">
      <header className="border-b border-line">
        <div className="mx-auto flex max-w-[1360px] flex-wrap items-center gap-7 px-4 py-3 sm:px-6">
          <Link to="/" aria-label={m.nav_home()} className="text-foreground no-underline">
            <Logo />
          </Link>
          <nav aria-label={m.nav_main()} className="flex flex-wrap gap-1">
            <Link
              to="/"
              className="rounded-sm px-3 py-2 text-sm font-medium text-muted-foreground no-underline hover:text-foreground"
              activeProps={{ className: 'bg-secondary !text-foreground' }}
            >
              {m.nav_forecast()}
            </Link>
          </nav>
        </div>
      </header>
      <Outlet />
      <footer className="mx-auto mt-auto w-full max-w-[1360px] px-4 sm:px-6">
        <div className="flex flex-wrap justify-between gap-3 border-t border-line pt-5 pb-6 text-xs text-muted-foreground">
          <span>
            <a href="https://open-meteo.com/" className="text-subtle hover:text-foreground">
              {m.footer_weatherData()}
            </a>{' '}
            (CC BY 4.0)
          </span>
          <span>
            {m.footer_openSource()} ·{' '}
            <a href={REPO} className="text-subtle hover:text-foreground">
              GitHub
            </a>
          </span>
          <LanguageSwitch />
        </div>
      </footer>
    </div>
  );
}

function NotFound() {
  return (
    <main className="mx-auto flex w-full max-w-[1360px] flex-col gap-3 px-4 py-16 sm:px-6">
      <h1 className="m-0 text-4xl font-bold tracking-[-0.05em]">{m.notFound_title()}</h1>
      <p className="m-0 text-muted-foreground">
        {m.notFound_text()}{' '}
        <Link to="/" className="text-subtle hover:text-foreground">
          {m.notFound_home()}
        </Link>
      </p>
    </main>
  );
}
