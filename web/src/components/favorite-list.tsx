/** The signed-in user's spots on the start page, each as a card at a glance (spot-card.tsx)
 * leading to its page; the pen opens its settings. Nothing without an account or spots. */
import { useQuery } from '@tanstack/react-query';
import { Star } from 'lucide-react';
import { useState } from 'react';
import type { Favorite } from '@/bindings/Favorite';
import { favoritesQuery, meQuery } from '@/lib/api';
import { m } from '@/paraglide/messages';
import { FavoriteDialog } from './favorite';
import { SpotCard } from './spot-card';

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
      <ul className="m-0 grid list-none gap-3 p-0 [grid-template-columns:repeat(auto-fill,minmax(300px,1fr))]">
        {favorites.data.map((f) => (
          <li key={f.id}>
            <SpotCard favorite={f} onEdit={() => setEditing(f)} />
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
