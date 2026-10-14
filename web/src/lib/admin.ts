/** Logic of the admin page without UI: its tabs, sizes in words, filtering accounts. */
import type { AdminUser } from '@/bindings/AdminUser';
import type { DiskUsage } from '@/bindings/DiskUsage';
import { formatNumber } from './format';
import { intlLocale } from './i18n';

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

/** How often a task runs, as a key and a number: every n minutes, hourly, daily, weekly. */
export function cadence(seconds: number): { unit: 'min' | 'hour' | 'day' | 'week'; n: number } {
  if (seconds >= 7 * 86_400) return { unit: 'week', n: 1 };
  if (seconds >= 86_400) return { unit: 'day', n: 1 };
  if (seconds >= 3_600) return { unit: 'hour', n: 1 };
  return { unit: 'min', n: Math.max(1, Math.round(seconds / 60)) };
}

/** How long something took: "0,8 s", "48 s", "3 min". */
export function took(ms: number): string {
  if (ms < 10_000) return `${formatNumber(ms / 1000, 1)} s`;
  if (ms < 120_000) return `${formatNumber(Math.round(ms / 1000))} s`;
  return `${formatNumber(Math.round(ms / 60_000))} min`;
}

/** A span of time in its largest whole unit: "40 Minuten", "6 Tage". */
export function span(seconds: number): string {
  const [unit, n] =
    seconds >= 2 * 86_400
      ? (['day', seconds / 86_400] as const)
      : seconds >= 2 * 3_600
        ? (['hour', seconds / 3_600] as const)
        : (['minute', Math.max(1, seconds / 60)] as const);
  return new Intl.NumberFormat(intlLocale(), { style: 'unit', unit, unitDisplay: 'long' }).format(Math.floor(n));
}

/** Share of a limit used, 0 to 100. */
export function percent(used: number, limit: number): number {
  return limit > 0 ? Math.min(100, Math.round((used / limit) * 100)) : 0;
}

/** Bytes the data folder takes, all parts together. */
export function diskUsed(d: DiskUsage): number {
  return d.models.reduce((a, [, b]) => a + b, 0) + d.basemap + d.terrain + d.spots + d.database + d.partial + d.other;
}

/** A Protomaps build (YYYYMMDD) as a date in words: "13.10.2026". */
export function buildDate(build: string): string {
  const m = /^(\d{4})(\d{2})(\d{2})$/.exec(build);
  if (!m) return build;
  return new Intl.DateTimeFormat(intlLocale(), { dateStyle: 'medium', timeZone: 'UTC' }).format(
    new Date(Date.UTC(Number(m[1]), Number(m[2]) - 1, Number(m[3]))),
  );
}

/** A Unix time as a date: "13.10.2026". */
export function date(unix: number): string {
  return new Intl.DateTimeFormat(intlLocale(), { dateStyle: 'medium' }).format(new Date(unix * 1000));
}

/** A region as typed: "west,south,east,north"; empty for none. */
export function regionText(r: [number, number, number, number] | null): string {
  return r ? r.join(',') : '';
}

/** A region from what was typed; undefined when it is no region, null for none. */
export function parseRegion(text: string): [number, number, number, number] | null | undefined {
  if (!text.trim() || text.trim() === 'none') return null;
  const v = text.split(',').map((p) => Number(p.trim()));
  if (v.length !== 4 || v.some((n) => !Number.isFinite(n))) return undefined;
  const [w, s, e, n] = v;
  if (w < -180 || e > 180 || s < -90 || n > 90 || w >= e || s >= n) return undefined;
  return [w, s, e, n];
}

/** Seconds a job still needs at its rate; null when it cannot be told. */
export function remaining(done: number, total: number, rate: number): number | null {
  if (rate <= 0 || total <= done) return null;
  return (total - done) / rate;
}

/** The number a setting field should send for `text`, or null when there is nothing to send:
 * not a whole number in `min`..`max`, or the value last sent. */
export function numberToSave(text: string, min: number, max: number, last: number): number | null {
  const n = Number(text.trim());
  if (text.trim() === '' || !Number.isInteger(n) || n < min || n > max || n === last) return null;
  return n;
}
