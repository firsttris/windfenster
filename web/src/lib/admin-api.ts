/** The admin page's requests; only admins get answers (`forbidden` else). */
import { queryOptions } from '@tanstack/react-query';
import type { AdminLogEntry } from '@/bindings/AdminLogEntry';
import type { AdminOverview } from '@/bindings/AdminOverview';
import type { AdminUserDetail } from '@/bindings/AdminUserDetail';
import type { AdminUsers } from '@/bindings/AdminUsers';
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
