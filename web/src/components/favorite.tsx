/** Keep a station or a point as a favourite: a star on its page; signed out it leads to the
 * sign-in and back. A new favourite opens its settings: an own name and the wind directions that
 * work there; a saved spot gets a bell for its alarm. */
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { Link, useRouterState } from '@tanstack/react-router';
import { Star } from 'lucide-react';
import { type FormEvent, useState } from 'react';
import type { Favorite } from '@/bindings/Favorite';
import type { Sector } from '@/bindings/Sector';
import { favoritesQuery, meQuery, send } from '@/lib/api';
import { COMPASS_DEGREES, type FavoriteTarget, findFavorite } from '@/lib/favorites';
import { compass } from '@/lib/format';
import { describeError } from '@/lib/i18n';
import { m } from '@/paraglide/messages';
import { Button } from '@/ui/button';
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '@/ui/dialog';
import { AlarmButton } from './alarm';

interface Props {
  target: FavoriteTarget;
  lat: number;
  lon: number;
  /** The name a new favourite starts with */
  name: string;
}

export function FavoriteButton({ target, lat, lon, name }: Props) {
  const me = useQuery(meQuery);
  const favorites = useQuery({ ...favoritesQuery, enabled: Boolean(me.data) });
  const queryClient = useQueryClient();
  const here = useRouterState({ select: (s) => s.location.href });
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const fav = favorites.data && findFavorite(favorites.data, target);

  if (me.isPending) return null;
  if (!me.data) {
    return (
      <Button variant="outline" asChild>
        <Link to="/login" search={{ redirect: here }}>
          <Star aria-hidden="true" />
          {m.favorite_add()}
        </Link>
      </Button>
    );
  }

  const add = async () => {
    setBusy(true);
    try {
      const body = { slug: 'slug' in target ? target.slug : null, lat, lon, name, sector: null };
      const added = await send<Favorite>('POST', '/favorites', body);
      queryClient.setQueryData<Favorite[]>(['favorites'], (l) => [...(l ?? []), added]);
      setOpen(true);
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <Button
        variant={fav ? 'secondary' : 'outline'}
        disabled={busy || favorites.isPending}
        aria-pressed={Boolean(fav)}
        onClick={() => (fav ? setOpen(true) : void add())}
      >
        <Star aria-hidden="true" className={fav ? 'fill-current text-amber-500' : undefined} />
        {fav ? m.favorite_saved() : m.favorite_add()}
      </Button>
      {fav && <AlarmButton favorite={fav} />}
      {fav && <FavoriteDialog favorite={fav} open={open} onOpenChange={setOpen} />}
    </>
  );
}

const SELECT =
  'h-10 rounded-md border bg-background px-2 text-base outline-none focus-visible:ring-2 focus-visible:ring-ring/60';

/** Name, wind directions and removal of a favourite. */
export function FavoriteDialog({
  favorite,
  open,
  onOpenChange,
}: {
  favorite: Favorite;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const queryClient = useQueryClient();
  const [name, setName] = useState(favorite.name);
  const [sector, setSector] = useState<Sector | null>(favorite.sector);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const names = m.compass_points().split(',');

  const run = async (action: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    try {
      await action();
      onOpenChange(false);
    } catch (e) {
      setError(describeError(e));
    } finally {
      setBusy(false);
    }
  };
  const save = (e: FormEvent) => {
    e.preventDefault();
    void run(async () => {
      const changed = await send<Favorite>('PUT', `/favorites/${favorite.id}`, { name, sector });
      queryClient.setQueryData<Favorite[]>(['favorites'], (l) => l?.map((f) => (f.id === changed.id ? changed : f)));
    });
  };
  const remove = () =>
    run(async () => {
      await send('DELETE', `/favorites/${favorite.id}`);
      queryClient.setQueryData<Favorite[]>(['favorites'], (l) => l?.filter((f) => f.id !== favorite.id));
    });
  const pick = (end: 'from' | 'to', deg: number) =>
    setSector((s) => ({ ...(s ?? { from: 225, to: 315 }), [end]: deg }));

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="p-5">
        <form onSubmit={save} className="flex flex-col gap-4">
          <div className="flex flex-col gap-1">
            <DialogTitle className="m-0 font-bold text-xl tracking-[-0.03em]">{m.favorite_title()}</DialogTitle>
            <DialogDescription className="m-0 text-muted-foreground text-sm">{m.favorite_lead()}</DialogDescription>
          </div>
          <label className="flex flex-col gap-1.5 font-medium text-sm">
            {m.favorite_name()}
            <input
              value={name}
              onChange={(e) => setName(e.target.value)}
              maxLength={60}
              required
              className="h-10 rounded-md border bg-background px-3 font-normal text-base outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
            />
          </label>
          <fieldset className="m-0 flex flex-col gap-2 border-0 p-0">
            <legend className="mb-1.5 p-0 font-medium text-sm">{m.favorite_directions()}</legend>
            <label className="flex items-center gap-2 text-sm">
              <input
                type="checkbox"
                checked={sector === null}
                onChange={(e) => setSector(e.target.checked ? null : { from: 225, to: 315 })}
                className="size-4"
              />
              {m.favorite_anyDirection()}
            </label>
            {sector && (
              <div className="flex items-center gap-4">
                <SectorRose sector={sector} />
                <div className="flex flex-col gap-2 text-sm">
                  <label className="flex items-center justify-between gap-2">
                    {m.favorite_from()}
                    <select
                      value={sector.from}
                      onChange={(e) => pick('from', Number(e.target.value))}
                      className={SELECT}
                    >
                      {COMPASS_DEGREES.map((d, i) => (
                        <option key={d} value={d}>
                          {names[i]} ({d}°)
                        </option>
                      ))}
                    </select>
                  </label>
                  <label className="flex items-center justify-between gap-2">
                    {m.favorite_to()}
                    <select value={sector.to} onChange={(e) => pick('to', Number(e.target.value))} className={SELECT}>
                      {COMPASS_DEGREES.map((d, i) => (
                        <option key={d} value={d}>
                          {names[i]} ({d}°)
                        </option>
                      ))}
                    </select>
                  </label>
                  <span className="text-muted-foreground text-xs">
                    {m.favorite_sectorText({ from: compass(sector.from), to: compass(sector.to) })}
                  </span>
                </div>
              </div>
            )}
          </fieldset>
          {error && (
            <p role="alert" className="m-0 text-destructive text-sm">
              {error}
            </p>
          )}
          <div className="flex flex-wrap justify-between gap-2">
            <Button type="button" variant="ghost" disabled={busy} onClick={() => void remove()}>
              {m.favorite_remove()}
            </Button>
            <Button type="submit" disabled={busy || !name.trim()}>
              {m.favorite_save()}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}

/** The sector as a slice of a compass rose, clockwise from `from` to `to` (wind comes from it). */
function SectorRose({ sector }: { sector: Sector }) {
  const r = 34;
  const at = (deg: number) => {
    const a = ((deg - 90) * Math.PI) / 180;
    return `${40 + r * Math.cos(a)} ${40 + r * Math.sin(a)}`;
  };
  const span = (sector.to - sector.from + 360) % 360;
  const path = `M40 40 L${at(sector.from)} A${r} ${r} 0 ${span > 180 ? 1 : 0} 1 ${at(sector.to)} Z`;
  return (
    <svg viewBox="0 0 80 80" className="size-20 shrink-0" aria-hidden="true">
      <circle cx="40" cy="40" r={r} className="fill-none stroke-line" strokeWidth="1.5" />
      <path d={path} className="fill-emerald-500/40 stroke-emerald-500" strokeWidth="1.5" />
      <text x="40" y="11" textAnchor="middle" className="fill-muted-foreground text-[9px]">
        {m.compass_points().split(',')[0]}
      </text>
    </svg>
  );
}
