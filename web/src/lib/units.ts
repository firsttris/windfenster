import type { WindUnit } from '@/bindings/WindUnit';
import { intlLocale } from './i18n';

/** Metres per second in one knot (same constant as wf_core::units). */
export const MS_PER_KN = 1852 / 3600;
/** Kilometres per hour in one knot. */
export const KMH_PER_KN = 1.852;

export const WIND_UNITS: readonly WindUnit[] = ['kn', 'ms', 'kmh'];

export const UNIT_LABEL: Record<WindUnit, string> = { kn: 'kn', ms: 'm/s', kmh: 'km/h' };

const oneDecimal = new Intl.NumberFormat(intlLocale(), { minimumFractionDigits: 1, maximumFractionDigits: 1 });
const whole = new Intl.NumberFormat(intlLocale(), { maximumFractionDigits: 0 });

/** Wind from knots (as the API sends it) in the chosen unit: m/s with one decimal, else whole. */
export function formatWind(kn: number, unit: WindUnit): string {
  switch (unit) {
    case 'kn':
      return whole.format(kn);
    case 'ms':
      return oneDecimal.format(kn * MS_PER_KN);
    case 'kmh':
      return whole.format(kn * KMH_PER_KN);
  }
}
