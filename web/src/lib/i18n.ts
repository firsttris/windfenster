/**
 * Languages. Every text lives in messages/{de,en}.json; Paraglide compiles them to
 * src/paraglide, one typed function per text (m.nav_forecast()). The language is the one
 * picked in the app (localStorage), else the browser's, else German. setLocale() reloads.
 *
 * The API sends codes, never texts: errorText and noticeText map each code to its message,
 * and TypeScript fails if a code has none.
 */
import type { ErrorCode } from '@/bindings/ErrorCode';
import type { Notice } from '@/bindings/Notice';
import { m } from '@/paraglide/messages';
import { getLocale, type Locale, locales, setLocale } from '@/paraglide/runtime';
import { isApiError } from './api';

export { getLocale, type Locale, locales, setLocale };

/** Each language in its own name, for the switcher. */
export const LOCALE_NAMES: Record<Locale, string> = { de: 'Deutsch', en: 'English' };

/** Intl locale for numbers and dates; English with the 24-hour clock (en-GB). */
export function intlLocale(locale: Locale = getLocale()): string {
  return locale === 'de' ? 'de-DE' : 'en-GB';
}

export const errorText: Record<ErrorCode, () => string> = {
  not_found: m.error_notFound,
  invalid_coordinates: m.error_invalidCoordinates,
  invalid_range: m.error_invalidRange,
  internal: m.error_internal,
};

export const noticeText: Record<Notice, () => string> = {
  source_unavailable: m.notice_sourceUnavailable,
  budget_exhausted: m.notice_budgetExhausted,
};

/** A failed request in words: the server's code, or that it could not be reached. */
export function describeError(error: unknown): string {
  if (isApiError(error) && error.code) return errorText[error.code]();
  return m.error_unreachable();
}
