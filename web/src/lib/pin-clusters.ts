/** Stations bundled from afar, as MapLibre expressions: a bundle shows the strongest wind
 * measured in it, in the wind scale's colours, so the map tells where it blows before you zoom
 * in (Windfinder's bundles only count stations). */
import type { ExpressionSpecification } from 'maplibre-gl';
import type { WindUnit } from '@/bindings/WindUnit';
import { WIND_SCALE } from '@/ui/wind-scale';
import { KMH_PER_KN, MS_PER_KN } from './units';

/** Up to this zoom stations near each other are bundled. */
export const CLUSTER_MAX_ZOOM = 6;
/** Pixels within which stations join a bundle */
export const CLUSTER_RADIUS = 40;
/** Wind of a station without a recent measurement: below every band */
export const NO_WIND = -1;

/** The band colour (`bg` or `fg`) of a wind in kn; `none` without a measurement. */
export function bandColor(kn: ExpressionSpecification, key: 'bg' | 'fg', none: string): ExpressionSpecification {
  const [first, ...rest] = WIND_SCALE;
  return [
    'case',
    ['<', kn, 0],
    none,
    ['step', kn, first[key], ...rest.flatMap((b) => [b.from, b[key]])],
  ] as ExpressionSpecification;
}

/** A wind in kn as text in the chosen unit, rounded as formatWind does. */
export function windText(kn: ExpressionSpecification, unit: WindUnit, locale: string): ExpressionSpecification {
  const factor = unit === 'ms' ? MS_PER_KN : unit === 'kmh' ? KMH_PER_KN : 1;
  const digits = unit === 'ms' ? 1 : 0;
  return ['number-format', ['*', kn, factor], { locale, 'min-fraction-digits': digits, 'max-fraction-digits': digits }];
}
