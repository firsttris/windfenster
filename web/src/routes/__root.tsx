import type { QueryClient } from '@tanstack/react-query';
import { createRootRouteWithContext, Link, Outlet } from '@tanstack/react-router';
import { Search } from 'lucide-react';
import { useEffect } from 'react';
import { SearchButton, SearchProvider, useOpenSearch } from '@/components/search';
import { watchTheme } from '@/lib/prefs';
import { m } from '@/paraglide/messages';
import { Button } from '@/ui/button';
import { LanguageSwitch } from '@/ui/language-switch';
import { Logo } from '@/ui/logo';
import { ThemeToggle } from '@/ui/theme-toggle';

export const Route = createRootRouteWithContext<{ queryClient: QueryClient }>()({
  component: RootLayout,
  notFoundComponent: NotFound,
});

const REPO = 'https://github.com/firsttris/windfenster';

function RootLayout() {
  useEffect(() => watchTheme(), []);
  return (
    <SearchProvider>
      <div className="flex min-h-dvh flex-col">
        <Header />
        <Outlet />
        <footer className="mx-auto mt-auto w-full max-w-[1360px] px-4 sm:px-6">
          <div className="flex flex-wrap items-center justify-between gap-3 border-line border-t pt-5 pb-6 text-muted-foreground text-xs">
            <span>
              <a href="https://open-meteo.com/" className="text-subtle hover:text-foreground">
                {m.footer_weatherData()}
              </a>{' '}
              (CC BY 4.0) · {m.footer_stations()}{' '}
              <a href="https://ourairports.com/data/" className="text-subtle hover:text-foreground">
                OurAirports
              </a>
              ,{' '}
              <a href="https://opendata.dwd.de/" className="text-subtle hover:text-foreground">
                DWD
              </a>
            </span>
            <span className="flex flex-wrap items-center gap-3">
              <span>
                {m.footer_openSource()} ·{' '}
                <a href={REPO} className="text-subtle hover:text-foreground">
                  GitHub
                </a>
              </span>
              <LanguageSwitch />
            </span>
          </div>
        </footer>
      </div>
    </SearchProvider>
  );
}

function Header() {
  const openSearch = useOpenSearch();
  return (
    <header className="border-line border-b">
      <div className="mx-auto flex max-w-[1360px] items-center gap-4 px-4 py-3 sm:gap-7 sm:px-6">
        <Link to="/" aria-label={m.nav_home()} className="text-foreground no-underline">
          <Logo />
        </Link>
        <nav aria-label={m.nav_main()} className="hidden gap-1 sm:flex">
          <Link
            to="/"
            className="rounded-sm px-3 py-2 font-medium text-muted-foreground text-sm no-underline hover:text-foreground"
            activeProps={{ className: 'bg-secondary !text-foreground' }}
          >
            {m.nav_forecast()}
          </Link>
        </nav>
        <div className="flex flex-1 items-center justify-end gap-2.5">
          <SearchButton className="hidden max-w-xs sm:flex" />
          <Button
            variant="outline"
            size="icon"
            className="sm:hidden"
            aria-label={m.search_title()}
            onClick={openSearch}
          >
            <Search aria-hidden="true" />
          </Button>
          <ThemeToggle />
        </div>
      </div>
    </header>
  );
}

function NotFound() {
  return (
    <main className="mx-auto flex w-full max-w-[1360px] flex-col gap-3 px-4 py-16 sm:px-6">
      <h1 className="m-0 font-bold text-4xl tracking-[-0.05em]">{m.notFound_title()}</h1>
      <p className="m-0 text-muted-foreground">
        {m.notFound_text()}{' '}
        <Link to="/" className="text-subtle hover:text-foreground">
          {m.notFound_home()}
        </Link>
      </p>
    </main>
  );
}
