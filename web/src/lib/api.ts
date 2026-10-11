import { queryOptions } from '@tanstack/react-query';
import type { ErrorBody } from '@/bindings/ErrorBody';
import type { Health } from '@/bindings/Health';
import type { Model } from '@/bindings/Model';

export class ApiError extends Error {
  readonly status: number;

  constructor(status: number, message: string) {
    super(message);
    this.status = status;
  }
}

/** GET on the Windfenster API; non-2xx answers throw an ApiError with the server's message. */
export async function api<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`/api${path}`, { ...init, headers: { accept: 'application/json', ...init?.headers } });
  if (!res.ok) {
    let message = res.statusText;
    try {
      message = ((await res.json()) as ErrorBody).error;
    } catch {
      // not JSON, e.g. a proxy error page
    }
    throw new ApiError(res.status, message);
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
