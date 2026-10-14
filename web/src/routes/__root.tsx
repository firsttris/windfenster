import { type QueryClient, useQuery } from '@tanstack/react-query';
import { createRootRouteWithContext, Link, Outlet, useRouterState } from '@tanstack/react-router';
import { LogIn, Map as MapIcon, Search, Settings, UserRound } from 'lucide-react';
import { useEffect } from 'react';
import { SearchButton, SearchProvider, useOpenSearch } from '@/components/search';
import { meQuery, siteQuery } from '@/lib/api';
import { watchTheme } from '@/lib/prefs';
import { cn } from '@/lib/utils';
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
  // The map fills the whole window, the header floats over it; its sources stand on the map itself.
  const fullscreen = useRouterState({ select: (s) => s.location.pathname === '/map' });
  return (
    <SearchProvider>
      <div
        className={
          fullscreen ? 'wf-fullscreen relative flex h-dvh flex-col overflow-hidden' : 'flex min-h-dvh flex-col'
        }
      >
        {!fullscreen && <Banner />}
        <Header over={fullscreen} />
        <Outlet />
        {!fullscreen && <Footer />}
      </div>
    </SearchProvider>
  );
}

/** The operator's notice above every page, when there is one (admin page, System). */
function Banner() {
  const site = useQuery(siteQuery);
  const text = site.data?.banner;
  if (!text) return null;
  return (
    <aside
      aria-label={m.banner_label()}
      className="border-warning-border border-b bg-warning-surface px-4 py-2 text-center text-[13px] text-warning-foreground"
    >
      {text}
    </aside>
  );
}

function Footer() {
  const site = useQuery(siteQuery);
  return (
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
          </a>{' '}
          · {m.footer_measurements()}{' '}
          <a href="https://aviationweather.gov/" className="text-subtle hover:text-foreground">
            aviationweather.gov
          </a>
          ,{' '}
          <a href="https://opendata.dwd.de/" className="text-subtle hover:text-foreground">
            DWD
          </a>{' '}
          ·{' '}
          <Link to="/sources" className="text-subtle hover:text-foreground">
            {m.footer_sources()}
          </Link>
        </span>
        <span className="flex flex-wrap items-center gap-3">
          {site.data?.imprint && (
            <Link to="/imprint" className="text-subtle hover:text-foreground">
              {m.footer_imprint()}
            </Link>
          )}
          {site.data?.privacy && (
            <Link to="/privacy" className="text-subtle hover:text-foreground">
              {m.footer_privacy()}
            </Link>
          )}
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
  );
}

/** The site's header; `over` the map it floats, see-through, 64 px high (the map's panels start
 * below it, routes/map.tsx). */
function Header({ over }: { over: boolean }) {
  const openSearch = useOpenSearch();
  // Over the map there is no bar: logo, menu and buttons float on it, each on its own ground, and
  // the map takes drags between them.
  const link = cn(
    'px-3 font-medium text-muted-foreground text-sm no-underline hover:text-foreground',
    over ? 'rounded-full py-1.5' : 'rounded-sm py-2',
  );
  return (
    <header
      className={
        over
          ? 'pointer-events-none absolute inset-x-0 top-0 z-40 [&_a]:pointer-events-auto [&_button]:pointer-events-auto'
          : 'border-line border-b'
      }
    >
      <div
        className={cn(
          'mx-auto flex items-center gap-2 px-3 min-[380px]:gap-3 min-[380px]:px-4',
          // over the map in line with its pills, 16 px from the edge; all of them 40 px high
          over ? 'h-16 sm:gap-4' : 'max-w-[1360px] py-3 sm:gap-7 sm:px-6',
        )}
      >
        <Link
          to="/"
          aria-label={m.nav_home()}
          className={cn(
            'text-foreground no-underline',
            over && 'glass flex h-10 items-center rounded-full border pr-3 pl-1 shadow-none min-[380px]:pr-4',
          )}
        >
          <Logo />
        </Link>
        <nav
          aria-label={m.nav_main()}
          className={cn('hidden gap-1 sm:flex', over && 'glass h-10 items-center rounded-full border p-1 shadow-none')}
        >
          <Link to="/" className={link} activeProps={{ className: 'bg-secondary !text-foreground' }}>
            {m.nav_forecast()}
          </Link>
          <Link to="/map" className={link} activeProps={{ className: 'bg-secondary !text-foreground' }}>
            {m.nav_map()}
          </Link>
        </nav>
        <div className="flex flex-1 items-center justify-end gap-1 min-[380px]:gap-1.5 sm:gap-2.5">
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
          <Button variant="outline" size="icon" className="sm:hidden" aria-label={m.nav_map()} asChild>
            <Link to="/map">
              <MapIcon aria-hidden="true" />
            </Link>
          </Button>
          <ThemeToggle />
          <AccountButton />
        </div>
      </div>
    </header>
  );
}

/** Signed in: the account, by name on wider screens; else the way to sign in. */
function AccountButton() {
  const me = useQuery(meQuery);
  if (me.isPending) return null;
  if (!me.data) {
    return (
      <Button variant="outline" className="max-md:size-10 max-md:px-0" asChild>
        <Link to="/login" aria-label={m.nav_signIn()}>
          <LogIn aria-hidden="true" />
          <span className="hidden md:inline">{m.nav_signIn()}</span>
        </Link>
      </Button>
    );
  }
  return (
    <>
      {me.data.admin && (
        <Button
          variant="outline"
          size="icon"
          className="max-sm:hidden"
          aria-label={m.nav_admin()}
          title={m.nav_admin()}
          asChild
        >
          <Link to="/admin">
            <Settings aria-hidden="true" />
          </Link>
        </Button>
      )}
      <AccountLink name={me.data.name} />
    </>
  );
}

function AccountLink({ name }: { name: string }) {
  return (
    <Button variant="outline" className="max-md:size-10 max-md:px-0" asChild>
      <Link to="/account" aria-label={m.nav_account({ name })}>
        <UserRound aria-hidden="true" />
        <span className="hidden max-w-32 truncate md:inline">{name}</span>
      </Link>
    </Button>
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
