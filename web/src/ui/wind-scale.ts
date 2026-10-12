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

/** Fourteen steps after Windfinder's order (decided with Tristan, 12.10.2026): calm violet, then
 * blue from dark to light, cyan, teal, green to lime, from 23 kn yellow, orange, red and from
 * 35 kn magenta; a little calmer than Windfinder's neon on our dark surfaces. The text colour is
 * the one of the two with more contrast (wind-scale.test.ts holds every step to 4.5:1). */
export const WIND_SCALE: readonly WindBand[] = [
  { from: 0, bg: '#4c2a85', fg: '#ffffff' },
  { from: 3, bg: '#4338ca', fg: '#ffffff' },
  { from: 5, bg: '#1d4ed8', fg: '#ffffff' },
  { from: 7, bg: '#2563eb', fg: '#ffffff' },
  { from: 9, bg: '#0284c7', fg: '#0b0f05' },
  { from: 11, bg: '#06a6c4', fg: '#0b0f05' },
  { from: 13, bg: '#0d9488', fg: '#0b0f05' },
  { from: 15, bg: '#16a34a', fg: '#0b0f05' },
  { from: 17, bg: '#22c55e', fg: '#0b0f05' },
  { from: 20, bg: '#84cc16', fg: '#0b0f05' },
  { from: 23, bg: '#eab308', fg: '#0b0f05' },
  { from: 26, bg: '#f97316', fg: '#0b0f05' },
  { from: 30, bg: '#dc2626', fg: '#ffffff' },
  { from: 35, bg: '#c026d3', fg: '#ffffff' },
];

/** The band a wind speed in knots falls into; calm or invalid values get the first band. */
export function windBand(kn: number): WindBand {
  for (let i = WIND_SCALE.length - 1; i > 0; i--) {
    if (kn >= WIND_SCALE[i].from) return WIND_SCALE[i];
  }
  return WIND_SCALE[0];
}
