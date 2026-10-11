/** Numbers, dates and places in the viewer's language. */
import { m } from '@/paraglide/messages';
import { compassIndex } from './forecast';
import { intlLocale } from './i18n';

const pad = (n: number) => String(n).padStart(2, '0');

/** A local date (YYYY-MM-DD) as weekday, e.g. "Sa" / "Samstag". */
export function weekday(date: string, style: 'short' | 'long'): string {
  return new Intl.DateTimeFormat(intlLocale(), { weekday: style, timeZone: 'UTC' }).format(
    new Date(`${date}T12:00:00Z`),
  );
}

/** A local date as day and month, e.g. "11. Okt." / "11 Oct". */
export function dayMonth(date: string): string {
  return new Intl.DateTimeFormat(intlLocale(), { day: 'numeric', month: 'short', timeZone: 'UTC' }).format(
    new Date(`${date}T12:00:00Z`),
  );
}

export const hour = (h: number) => pad(h);

export function formatNumber(n: number, digits = 0): string {
  return new Intl.NumberFormat(intlLocale(), {
    minimumFractionDigits: digits,
    maximumFractionDigits: digits,
  }).format(n);
}

/** "48,78° N · 8,08° E" */
export function coordinates(lat: number, lon: number): string {
  const ns = lat >= 0 ? 'N' : 'S';
  const ew = lon >= 0 ? 'E' : 'W';
  return `${formatNumber(Math.abs(lat), 2)}° ${ns} · ${formatNumber(Math.abs(lon), 2)}° ${ew}`;
}

export function km(distance: number): string {
  return `${formatNumber(distance, distance < 10 ? 1 : 0)} km`;
}

/** "vor 38 Min." / "38 min ago"; under a minute "gerade eben" / "just now". */
export function ago(unix: number, now = Date.now() / 1000): string {
  const minutes = Math.round((now - unix) / 60);
  if (minutes < 1) return m.time_justNow();
  const rtf = new Intl.RelativeTimeFormat(intlLocale(), { style: 'short', numeric: 'always' });
  if (minutes < 60) return rtf.format(-minutes, 'minute');
  const hours = Math.round(minutes / 60);
  if (Math.abs(hours) < 48) return rtf.format(-hours, 'hour');
  return rtf.format(-Math.round(hours / 24), 'day');
}

/** The UTC hour of a model run, e.g. "12 UTC". */
export function runHour(unix: number): string {
  return `${pad(new Date(unix * 1000).getUTCHours())} UTC`;
}

/** A country's name from its ISO code, in the viewer's language. */
export function countryName(code: string | null): string | null {
  if (!code) return null;
  try {
    return new Intl.DisplayNames([intlLocale()], { type: 'region' }).of(code) ?? code;
  } catch {
    return code;
  }
}

/** "SW", "NNO" … in the viewer's language. */
export function compass(deg: number): string {
  return m.compass_points().split(',')[compassIndex(deg)] ?? '';
}
