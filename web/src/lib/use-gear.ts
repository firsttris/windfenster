/**
 * The user's gear: with an account on the server (the same on every device), without one in
 * this browser. Gear kept in the browser goes to the account at the first sign-in that finds
 * none there.
 */
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useEffect, useMemo, useRef } from 'react';
import type { GearItem } from '@/bindings/GearItem';
import { api, meQuery, send } from './api';
import { GEAR_KINDS } from './gear';
import { usePref } from './prefs';

const KEY = 'wf.gear';

function parse(text: string | null): GearItem[] {
  try {
    const v: unknown = JSON.parse(text ?? '[]');
    return Array.isArray(v)
      ? v.filter(
          (g): g is GearItem =>
            GEAR_KINDS.includes(g?.kind) &&
            [g.size, g.minKn, g.maxKn].every((n) => typeof n === 'number') &&
            g.minKn < g.maxKn,
        )
      : [];
  } catch {
    return [];
  }
}

export function useGear(): { gear: GearItem[]; save: (g: GearItem[]) => Promise<void>; ready: boolean } {
  const me = useQuery(meQuery);
  const signedIn = Boolean(me.data);
  const queryClient = useQueryClient();
  const server = useQuery({
    queryKey: ['gear'],
    queryFn: () => api<GearItem[]>('/gear'),
    enabled: signedIn,
    staleTime: 5 * 60_000,
  });
  const [stored, setStored] = usePref(KEY);
  const local = useMemo(() => parse(stored), [stored]);

  // Gear kept in this browser goes to an account that has none yet, once.
  const moved = useRef(false);
  useEffect(() => {
    if (moved.current || !signedIn || !server.data || server.data.length > 0 || local.length === 0) return;
    moved.current = true;
    void send<GearItem[]>('PUT', '/gear', local).then((saved) => {
      queryClient.setQueryData(['gear'], saved);
      setStored(null);
    });
  }, [signedIn, server.data, local, queryClient, setStored]);

  if (signedIn) {
    return {
      gear: server.data ?? [],
      ready: !server.isPending,
      save: async (g) => {
        queryClient.setQueryData(['gear'], await send<GearItem[]>('PUT', '/gear', g));
      },
    };
  }
  return {
    gear: local,
    ready: !me.isPending,
    save: async (g) => setStored(JSON.stringify(g)),
  };
}
