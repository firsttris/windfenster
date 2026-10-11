import { queryOptions } from '@tanstack/react-query';
import type { ErrorBody } from '@/bindings/ErrorBody';
import type { ErrorCode } from '@/bindings/ErrorCode';
import type { Health } from '@/bindings/Health';
import type { Model } from '@/bindings/Model';

export class ApiError extends Error {
  readonly status: number;
  /** The server's code, or `null` when the answer came from somewhere else (proxy, network). */
  readonly code: ErrorCode | null;

  constructor(status: number, code: ErrorCode | null) {
    super(code ?? `HTTP ${status}`);
    this.status = status;
    this.code = code;
  }
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
    throw new ApiError(res.status, code);
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
