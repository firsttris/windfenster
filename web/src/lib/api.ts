import { queryOptions } from '@tanstack/react-query';
import type { Account } from '@/bindings/Account';
import type { Alarm } from '@/bindings/Alarm';
import type { ErrorBody } from '@/bindings/ErrorBody';
import type { ErrorCode } from '@/bindings/ErrorCode';
import type { Favorite } from '@/bindings/Favorite';
import type { Health } from '@/bindings/Health';
import type { MapIndex } from '@/bindings/MapIndex';
import type { MapSeries } from '@/bindings/MapSeries';
import type { Model } from '@/bindings/Model';
import type { NearGauge } from '@/bindings/NearGauge';
import type { PointForecast } from '@/bindings/PointForecast';
import type { PushInfo } from '@/bindings/PushInfo';
import type { SearchResults } from '@/bindings/SearchResults';
import type { StationForecast } from '@/bindings/StationForecast';
import type { StationHistory } from '@/bindings/StationHistory';
import type { StationLive } from '@/bindings/StationLive';
import type { StationWind } from '@/bindings/StationWind';

/** A failed API request: the HTTP status and the server's code (`null` when the answer came
 * from somewhere else, e.g. a proxy). Thrown, because TanStack Query expects rejections. */
export interface ApiError extends Error {
  name: 'ApiError';
  status: number;
  code: ErrorCode | null;
}

export function apiError(status: number, code: ErrorCode | null): ApiError {
  return Object.assign(new Error(code ?? `HTTP ${status}`), { name: 'ApiError' as const, status, code });
}

export function isApiError(e: unknown): e is ApiError {
  return e instanceof Error && e.name === 'ApiError';
}

/** A request to the Windfenster API; non-2xx answers throw an ApiError with the server's code,
 * an answer without content (204) is undefined. */
export async function api<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`/api${path}`, { ...init, headers: { accept: 'application/json', ...init?.headers } });
  if (!res.ok) {
    let code: ErrorCode | null = null;
    try {
      code = ((await res.json()) as ErrorBody).code;
    } catch {
      // not JSON, e.g. a proxy error page
    }
    throw apiError(res.status, code);
  }
  if (res.status === 204) return undefined as T;
  return (await res.json()) as T;
}

/** POST, PUT, PATCH or DELETE with a JSON body. */
export function send<T>(method: 'POST' | 'PUT' | 'PATCH' | 'DELETE', path: string, body?: unknown): Promise<T> {
  return api<T>(path, {
    method,
    headers: body === undefined ? undefined : { 'content-type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
}

/** The signed-in account, or null */
export const meQuery = queryOptions({
  queryKey: ['me'],
  queryFn: () => api<Account | null>('/me'),
  staleTime: 5 * 60_000,
});

export const alarmsQuery = queryOptions({
  queryKey: ['alarms'],
  queryFn: () => api<Alarm[]>('/alarms'),
  staleTime: 5 * 60_000,
});

/** What the server can send: its push key, and whether e-mail is set up */
export const pushInfoQuery = queryOptions({
  queryKey: ['push'],
  queryFn: () => api<PushInfo>('/push'),
  staleTime: 60 * 60_000,
});

export const favoritesQuery = queryOptions({
  queryKey: ['favorites'],
  queryFn: () => api<Favorite[]>('/favorites'),
  staleTime: 5 * 60_000,
});

export const healthQuery = queryOptions({
  queryKey: ['health'],
  queryFn: () => api<Health>('/health'),
  refetchInterval: 30_000,
});

export const modelsQuery = queryOptions({
  queryKey: ['models'],
  queryFn: () => api<Model[]>('/models'),
  staleTime: Infinity,
});

export const searchQuery = (q: string, lang: string) =>
  queryOptions({
    queryKey: ['search', q, lang],
    queryFn: ({ signal }) =>
      api<SearchResults>(`/stations?q=${encodeURIComponent(q)}&lang=${encodeURIComponent(lang)}`, { signal }),
    enabled: q.trim().length >= 2,
    staleTime: 5 * 60_000,
  });

export const stationQuery = (slug: string) =>
  queryOptions({
    queryKey: ['station', slug],
    queryFn: () => api<StationForecast>(`/stations/${encodeURIComponent(slug)}`),
    // A notice means the source failed: try again soon.
    refetchInterval: (q) => (q.state.data?.forecast.notice ? 60_000 : 10 * 60_000),
  });

export const pointQuery = (lat: number, lon: number) =>
  queryOptions({
    queryKey: ['point', lat.toFixed(2), lon.toFixed(2)],
    queryFn: () => api<PointForecast>(`/point?lat=${lat.toFixed(4)}&lon=${lon.toFixed(4)}`),
    refetchInterval: (q) => (q.state.data?.forecast.notice ? 60_000 : 10 * 60_000),
  });

export const liveQuery = (slug: string) =>
  queryOptions({
    queryKey: ['live', slug],
    queryFn: () => api<StationLive>(`/stations/${encodeURIComponent(slug)}/live`),
    // METARs come every 30 min, DWD values about every 30 min.
    refetchInterval: 2 * 60_000,
  });

export const gaugeQuery = (lat: number, lon: number) =>
  queryOptions({
    queryKey: ['gauge', lat.toFixed(2), lon.toFixed(2)],
    queryFn: () => api<NearGauge | null>(`/gauges/near?lat=${lat.toFixed(4)}&lon=${lon.toFixed(4)}`),
    // PEGELONLINE publishes every 15 minutes.
    refetchInterval: 15 * 60_000,
  });

export const historyQuery = (slug: string, from: number, to: number, live: boolean) =>
  queryOptions({
    queryKey: ['history', slug, from, to],
    queryFn: () => api<StationHistory>(`/stations/${encodeURIComponent(slug)}/history?from=${from}&to=${to}`),
    // Past days only change by a late backfill.
    staleTime: live ? 60_000 : 60 * 60_000,
  });

export const mapIndexQuery = queryOptions({
  queryKey: ['map'],
  queryFn: () => api<MapIndex>('/map'),
  // New hours of a run arrive every few minutes.
  refetchInterval: 5 * 60_000,
});

/** A place over every hour on the map, read from the model fields (no Open-Meteo call) */
export const mapSeriesQuery = (lat: number, lon: number) =>
  queryOptions({
    // 0.01°: the grid places are stored on
    queryKey: ['map', 'series', lat.toFixed(2), lon.toFixed(2)],
    queryFn: () => api<MapSeries>(`/map/series?lat=${lat.toFixed(2)}&lon=${lon.toFixed(2)}`),
    staleTime: 5 * 60_000,
  });

export const mapStationsQuery = queryOptions({
  queryKey: ['map', 'stations'],
  queryFn: () => api<StationWind[]>('/map/stations'),
  refetchInterval: 5 * 60_000,
});
