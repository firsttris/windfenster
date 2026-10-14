/** Logic of the admin page without UI: its tabs, sizes in words, filtering accounts. */
import type { AdminUser } from '@/bindings/AdminUser';
import { formatNumber } from './format';

export const ADMIN_TABS = [
  'overview',
  'map',
  'forecasts',
  'measurements',
  'users',
  'alarms',
  'storage',
  'system',
] as const;
export type AdminTab = (typeof ADMIN_TABS)[number];

/** The tab named in the address, else the first. */
export function parseTab(value: unknown, available: readonly AdminTab[] = ADMIN_TABS): AdminTab {
  return available.find((t) => t === value) ?? available[0];
}

/** Bytes in the largest unit that keeps the number at or above 1: "323 MB", "3,9 GB". */
export function bytes(n: number): string {
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let v = n;
  let u = 0;
  while (v >= 1000 && u < units.length - 1) {
    v /= 1000;
    u++;
  }
  return `${formatNumber(v, u === 0 || v >= 100 ? 0 : 1)} ${units[u]}`;
}

export type UserFilter = 'all' | 'disabled' | 'inactive';

/** Six months, in seconds: an account not seen for that long counts as inactive. */
const INACTIVE_S = 182 * 86_400;

/** The accounts a search and a filter leave, in the given order. */
export function filterUsers(users: AdminUser[], query: string, filter: UserFilter, now: number): AdminUser[] {
  const q = query.trim().toLowerCase();
  return users.filter((u) => {
    if (q && !u.name.toLowerCase().includes(q) && !(u.email ?? '').includes(q)) return false;
    if (filter === 'disabled') return u.disabledAt !== null;
    if (filter === 'inactive') return now - (u.lastSeenAt ?? u.createdAt) > INACTIVE_S;
    return true;
  });
}

/** How an account signs in. */
export function signInKind(u: Pick<AdminUser, 'passkeys' | 'password'>): 'passkey' | 'password' | 'both' | 'none' {
  if (u.passkeys > 0 && u.password) return 'both';
  if (u.passkeys > 0) return 'passkey';
  if (u.password) return 'password';
  return 'none';
}
