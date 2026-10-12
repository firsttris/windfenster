/** The signed-in user's spots on the start page: each opens its station or point page; the pen
 * opens its settings. Nothing without an account or spots. */
import { useQuery } from '@tanstack/react-query';
import { Link } from '@tanstack/react-router';
import { Pencil, Star } from 'lucide-react';
import { useState } from 'react';
import type { Favorite } from '@/bindings/Favorite';
import { favoritesQuery, meQuery } from '@/lib/api';
import { compass, coordinates } from '@/lib/format';
import { m } from '@/paraglide/messages';
import { FavoriteDialog } from './favorite';

export function FavoriteList() {
  const me = useQuery(meQuery);
  const favorites = useQuery({ ...favoritesQuery, enabled: Boolean(me.data) });
  const [editing, setEditing] = useState<Favorite | null>(null);
  if (!me.data || !favorites.data?.length) return null;
  return (
    <section aria-labelledby="favorites" className="flex flex-col gap-3">
      <h2 id="favorites" className="m-0 flex items-center gap-1.5 font-medium text-[13px] text-muted-foreground">
        <Star aria-hidden="true" className="size-3.5 fill-current text-amber-500" />
        {m.home_favorites()}
      </h2>
      <ul className="m-0 grid list-none gap-3 p-0 [grid-template-columns:repeat(auto-fill,minmax(240px,1fr))]">
        {favorites.data.map((f) => (
          <li key={f.id} className="relative">
            <Link
              {...(f.slug
                ? { to: '/station/$slug', params: { slug: f.slug } }
                : { to: '/point', search: { lat: f.lat, lon: f.lon, name: f.name } })}
              className="flex h-full flex-col gap-1 rounded-xl border bg-card p-4 pr-12 text-foreground no-underline transition-colors hover:border-border-strong"
            >
              <span className="truncate font-bold text-lg tracking-[-0.03em]">{f.name}</span>
              <span className="font-mono text-muted-foreground text-xs">
                {f.slug ? `${m.home_station()} ${f.slug.toUpperCase()}` : coordinates(f.lat, f.lon)}
              </span>
              {f.sector && (
                <span className="text-muted-foreground text-xs">
                  {m.favorite_sectorText({ from: compass(f.sector.from), to: compass(f.sector.to) })}
                </span>
              )}
            </Link>
            <button
              type="button"
              aria-label={`${m.home_favoritesEdit()}: ${f.name}`}
              onClick={() => setEditing(f)}
              className="absolute top-3 right-3 inline-flex size-8 cursor-pointer items-center justify-center rounded-md text-muted-foreground outline-none hover:bg-secondary hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/60"
            >
              <Pencil aria-hidden="true" className="size-4" />
            </button>
          </li>
        ))}
      </ul>
      {editing && (
        <FavoriteDialog
          key={editing.id}
          favorite={editing}
          open
          onOpenChange={(open) => {
            if (!open) setEditing(null);
          }}
        />
      )}
    </section>
  );
}
