import { queryOptions } from '@tanstack/react-query';
import type { ErrorBody } from '@/bindings/ErrorBody';
import type { ErrorCode } from '@/bindings/ErrorCode';
import type { Health } from '@/bindings/Health';
import type { Model } from '@/bindings/Model';

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

/** GET on the Windfenster API; non-2xx answers throw an ApiError with the server's code. */
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
  return (await res.json()) as T;
}

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
