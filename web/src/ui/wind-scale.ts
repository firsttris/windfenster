/**
 * The wind colour scale from the mockup, in knots. One constant for the forecast table, the
 * map's colour field, the legend and the alarm previews.
 */
export interface WindBand {
  /** Lower edge in kn, inclusive; the band runs up to the next band's edge. */
  from: number;
  /** Cell background. */
  bg: string;
  /** Text on the cell, readable on `bg`. */
  fg: string;
}

export const WIND_SCALE: readonly WindBand[] = [
  { from: 0, bg: '#1c2433', fg: '#a7b4c7' },
  { from: 5, bg: '#1e4a6b', fg: '#e0f2fe' },
  { from: 9, bg: '#2a9d8f', fg: '#04201c' },
  { from: 13, bg: '#8bd450', fg: '#0f1f05' },
  { from: 17, bg: '#f2d544', fg: '#2a2000' },
  { from: 21, bg: '#f5a03a', fg: '#2a1300' },
  { from: 25, bg: '#e8573c', fg: '#1f0700' },
  { from: 30, bg: '#b23a8f', fg: '#ffffff' },
];

/** The band a wind speed in knots falls into; calm or invalid values get the first band. */
export function windBand(kn: number): WindBand {
  for (let i = WIND_SCALE.length - 1; i > 0; i--) {
    if (kn >= WIND_SCALE[i].from) return WIND_SCALE[i];
  }
  return WIND_SCALE[0];
}
