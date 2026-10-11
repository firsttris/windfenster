import { useQuery } from '@tanstack/react-query';
import { useNavigate } from '@tanstack/react-router';
import { MapPin, Search, TowerControl } from 'lucide-react';
import { createContext, type ReactNode, useContext, useEffect, useState } from 'react';
import type { PlaceHit } from '@/bindings/PlaceHit';
import type { Station } from '@/bindings/Station';
import { searchQuery } from '@/lib/api';
import { parseCoordinates } from '@/lib/coords';
import { coordinates, countryName } from '@/lib/format';
import { describeError, getLocale } from '@/lib/i18n';
import { cn } from '@/lib/utils';
import { m } from '@/paraglide/messages';
import { Command, CommandGroup, CommandInput, CommandItem, CommandList } from '@/ui/command';
import { Dialog, DialogContent, DialogTitle } from '@/ui/dialog';

const SearchContext = createContext<() => void>(() => {});

/** Opens the search: the header button, ⌘K / Ctrl+K anywhere, or useOpenSearch(). */
export function SearchProvider({ children }: { children: ReactNode }) {
  const [open, setOpen] = useState(false);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault();
        setOpen((o) => !o);
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);
  return (
    <SearchContext.Provider value={() => setOpen(true)}>
      {children}
      <SearchDialog open={open} onOpenChange={setOpen} />
    </SearchContext.Provider>
  );
}

export const useOpenSearch = () => useContext(SearchContext);

const isMac = () => typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform);

/** Looks like a search field, opens the dialog. */
export function SearchButton({ className, large = false }: { className?: string; large?: boolean }) {
  const open = useOpenSearch();
  return (
    <button
      type="button"
      onClick={open}
      className={cn(
        'flex w-full cursor-pointer items-center gap-2.5 rounded-md border bg-card px-3 text-left text-muted-foreground text-sm outline-none transition-colors hover:border-border-strong focus-visible:ring-2 focus-visible:ring-ring/60',
        large ? 'h-14 rounded-xl px-4 text-base' : 'h-10',
        className,
      )}
    >
      <Search className="size-4 shrink-0" aria-hidden="true" />
      <span className="flex-1 truncate">{m.search_placeholder()}</span>
      <kbd className="hidden rounded-[5px] border px-1.5 py-0.5 font-mono text-[11px] sm:inline">
        {isMac() ? '⌘K' : 'Ctrl K'}
      </kbd>
    </button>
  );
}

function useDebounced<T>(value: T, ms: number): T {
  const [v, setV] = useState(value);
  useEffect(() => {
    const t = setTimeout(() => setV(value), ms);
    return () => clearTimeout(t);
  }, [value, ms]);
  return v;
}

function SearchDialog({ open, onOpenChange }: { open: boolean; onOpenChange: (o: boolean) => void }) {
  const [text, setText] = useState('');
  const q = useDebounced(text.trim(), 200);
  const results = useQuery(searchQuery(q, getLocale()));
  const navigate = useNavigate();
  const point = parseCoordinates(text);

  const close = () => {
    onOpenChange(false);
    setText('');
  };
  const toStation = (s: Station) => {
    close();
    void navigate({ to: '/station/$slug', params: { slug: s.slug } });
  };
  const toPoint = (lat: number, lon: number, name?: string) => {
    close();
    void navigate({ to: '/point', search: { lat, lon, name } });
  };

  const stations = q.length >= 2 ? (results.data?.stations ?? []) : [];
  const places = q.length >= 2 ? (results.data?.places ?? []) : [];
  const searching = q.length >= 2 && (results.isFetching || q !== text.trim());
  const nothing = q.length >= 2 && !searching && !results.isError && !point && !stations.length && !places.length;

  return (
    <Dialog open={open} onOpenChange={(o) => (o ? onOpenChange(true) : close())}>
      <DialogContent aria-describedby={undefined}>
        <DialogTitle className="sr-only">{m.search_title()}</DialogTitle>
        <Command shouldFilter={false} label={m.search_title()} loop>
          <CommandInput value={text} onValueChange={setText} placeholder={m.search_placeholder()} />
          <CommandList>
            {text.trim().length < 2 && <p className="m-0 px-3 py-6 text-muted-foreground text-sm">{m.search_hint()}</p>}
            {point && (
              <CommandGroup heading={m.search_point()}>
                <CommandItem value="point" onSelect={() => toPoint(point.lat, point.lon)}>
                  <MapPin className="size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
                  <span className="font-mono">{coordinates(point.lat, point.lon)}</span>
                </CommandItem>
              </CommandGroup>
            )}
            {stations.length > 0 && (
              <CommandGroup heading={m.search_stations()}>
                {stations.map((s) => (
                  <CommandItem key={s.slug} value={`station-${s.slug}`} onSelect={() => toStation(s)}>
                    <TowerControl className="size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
                    <span className="flex min-w-0 flex-1 flex-col">
                      <span className="truncate font-medium">{s.name}</span>
                      <span className="truncate text-muted-foreground text-xs">
                        {[s.municipality, s.region, countryName(s.country)].filter(Boolean).join(' · ')}
                      </span>
                    </span>
                    <span className="rounded-sm border px-1.5 py-0.5 font-mono text-[11px] text-muted-foreground">
                      {s.code}
                    </span>
                  </CommandItem>
                ))}
              </CommandGroup>
            )}
            {places.length > 0 && (
              <CommandGroup heading={m.search_places()}>
                {places.map((p: PlaceHit) => (
                  <CommandItem
                    key={`${p.lat},${p.lon}`}
                    value={`place-${p.lat},${p.lon}`}
                    onSelect={() => toPoint(p.lat, p.lon, p.name)}
                  >
                    <MapPin className="size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
                    <span className="flex min-w-0 flex-1 flex-col">
                      <span className="truncate font-medium">{p.name}</span>
                      <span className="truncate text-muted-foreground text-xs">
                        {[p.admin, countryName(p.country)].filter(Boolean).join(' · ')}
                      </span>
                    </span>
                  </CommandItem>
                ))}
              </CommandGroup>
            )}
            {searching && !stations.length && !places.length && (
              <p className="m-0 px-3 py-6 text-muted-foreground text-sm">{m.search_loading()}</p>
            )}
            {results.isError && q.length >= 2 && (
              <p className="m-0 px-3 py-6 text-destructive text-sm">{describeError(results.error)}</p>
            )}
            {nothing && <p className="m-0 px-3 py-6 text-muted-foreground text-sm">{m.search_none({ query: q })}</p>}
          </CommandList>
        </Command>
      </DialogContent>
    </Dialog>
  );
}
