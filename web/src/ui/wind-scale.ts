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

/** Windfinder's scale, as its stylesheet has it (classes ws0 to ws26 on cdn.windfinder.net, one
 * per 2 kn, decided with Tristan, 12.10.2026) and as its dark design shows it, at 80 % of the
 * brightness: violet in calm, blue from dark to light, cyan, teal, green, yellow from 26 kn,
 * orange from 30, red from 38, wine to magenta from 48. The text colour is the one of the two with
 * more contrast (wind-scale.test.ts holds every step to 4.5:1). */
export const WIND_SCALE: readonly WindBand[] = [
  { from: 0, bg: '#7900cc', fg: '#ffffff' },
  { from: 2, bg: '#5000cc', fg: '#ffffff' },
  { from: 4, bg: '#2800cc', fg: '#ffffff' },
  { from: 6, bg: '#0028cc', fg: '#ffffff' },
  { from: 8, bg: '#0050cc', fg: '#ffffff' },
  { from: 10, bg: '#0078cc', fg: '#ffffff' },
  { from: 12, bg: '#009fcc', fg: '#0b0f05' },
  { from: 14, bg: '#00b8c0', fg: '#0b0f05' },
  { from: 16, bg: '#1e9a75', fg: '#0b0f05' },
  { from: 18, bg: '#0eaa0e', fg: '#0b0f05' },
  { from: 20, bg: '#00b800', fg: '#0b0f05' },
  { from: 22, bg: '#00c800', fg: '#0b0f05' },
  { from: 24, bg: '#93cc4e', fg: '#0b0f05' },
  { from: 26, bg: '#cccb00', fg: '#0b0f05' },
  { from: 28, bg: '#ccb400', fg: '#0b0f05' },
  { from: 30, bg: '#cca000', fg: '#0b0f05' },
  { from: 32, bg: '#cc8c00', fg: '#0b0f05' },
  { from: 34, bg: '#cc7800', fg: '#0b0f05' },
  // Windfinder's #e67d00 a touch lighter, so dark text keeps 4.5:1 on it
  { from: 36, bg: '#bc6800', fg: '#0b0f05' },
  { from: 38, bg: '#b85000', fg: '#ffffff' },
  { from: 40, bg: '#b03b17', fg: '#ffffff' },
  { from: 42, bg: '#a02817', fg: '#ffffff' },
  { from: 44, bg: '#901417', fg: '#ffffff' },
  { from: 46, bg: '#880017', fg: '#ffffff' },
  { from: 48, bg: '#900028', fg: '#ffffff' },
  { from: 50, bg: '#a00050', fg: '#ffffff' },
  { from: 52, bg: '#cb0078', fg: '#ffffff' },
];

/** The band a wind speed in knots falls into; calm or invalid values get the first band. */
export function windBand(kn: number): WindBand {
  for (let i = WIND_SCALE.length - 1; i > 0; i--) {
    if (kn >= WIND_SCALE[i].from) return WIND_SCALE[i];
  }
  return WIND_SCALE[0];
}
