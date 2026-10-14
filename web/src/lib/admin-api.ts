/** The admin page's requests; only admins get answers (`forbidden` else). */
import { queryOptions } from '@tanstack/react-query';
import type { AdminAlarms } from '@/bindings/AdminAlarms';
import type { AdminForecasts } from '@/bindings/AdminForecasts';
import type { AdminLogEntry } from '@/bindings/AdminLogEntry';
import type { AdminMap } from '@/bindings/AdminMap';
import type { AdminMeasurements } from '@/bindings/AdminMeasurements';
import type { AdminOverview } from '@/bindings/AdminOverview';
import type { AdminSettings } from '@/bindings/AdminSettings';
import type { AdminStorage } from '@/bindings/AdminStorage';
import type { AdminSystem } from '@/bindings/AdminSystem';
import type { AdminUserDetail } from '@/bindings/AdminUserDetail';
import type { AdminUsers } from '@/bindings/AdminUsers';
import type { Invite } from '@/bindings/Invite';
import type { LogLine } from '@/bindings/LogLine';
import { api } from './api';

export const adminUsersQuery = queryOptions({
  queryKey: ['admin', 'users'],
  queryFn: () => api<AdminUsers>('/admin/users'),
});

export const adminUserQuery = (id: number) =>
  queryOptions({
    queryKey: ['admin', 'users', id],
    queryFn: () => api<AdminUserDetail>(`/admin/users/${id}`),
  });

export const adminLogQuery = queryOptions({
  queryKey: ['admin', 'log'],
  queryFn: () => api<AdminLogEntry[]>('/admin/log'),
});

export const adminOverviewQuery = queryOptions({
  queryKey: ['admin', 'overview'],
  queryFn: () => api<AdminOverview>('/admin/overview'),
  refetchInterval: 15_000,
});

export const adminSettingsQuery = queryOptions({
  queryKey: ['admin', 'settings'],
  queryFn: () => api<AdminSettings>('/admin/settings'),
});

export const adminMapQuery = queryOptions({
  queryKey: ['admin', 'map'],
  queryFn: () => api<AdminMap>('/admin/map'),
  // A cut under way moves on
  refetchInterval: (q) => ((q.state.data?.basemap.work.length ?? 0) > 0 ? 5_000 : 30_000),
});

export const adminForecastsQuery = queryOptions({
  queryKey: ['admin', 'forecasts'],
  queryFn: () => api<AdminForecasts>('/admin/forecasts'),
  refetchInterval: 60_000,
});

export const adminMeasurementsQuery = queryOptions({
  queryKey: ['admin', 'measurements'],
  queryFn: () => api<AdminMeasurements>('/admin/measurements'),
  refetchInterval: 60_000,
});

export const adminInvitesQuery = queryOptions({
  queryKey: ['admin', 'invites'],
  queryFn: () => api<Invite[]>('/admin/invites'),
});

export const adminAlarmsQuery = queryOptions({
  queryKey: ['admin', 'alarms'],
  queryFn: () => api<AdminAlarms>('/admin/alarms'),
  refetchInterval: 60_000,
});

export const adminStorageQuery = queryOptions({
  queryKey: ['admin', 'storage'],
  queryFn: () => api<AdminStorage>('/admin/storage'),
});

export const adminSystemQuery = queryOptions({
  queryKey: ['admin', 'system'],
  queryFn: () => api<AdminSystem>('/admin/system'),
});

export const adminLogLinesQuery = queryOptions({
  queryKey: ['admin', 'log-lines'],
  queryFn: () => api<LogLine[]>('/admin/log/lines'),
  refetchInterval: 30_000,
});
