import { describe, expect, it } from 'vitest';
import { WIND_SCALE, windBand } from './wind-scale';

/** WCAG contrast of two #rrggbb colours */
function contrast(a: string, b: string): number {
  const lum = (h: string) => {
    const [r, g, bl] = [1, 3, 5].map((i) => {
      const v = Number.parseInt(h.slice(i, i + 2), 16) / 255;
      return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
    });
    return 0.2126 * r + 0.7152 * g + 0.0722 * bl;
  };
  const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p);
  return (x + 0.05) / (y + 0.05);
}

describe('windBand', () => {
  it('uses inclusive lower edges', () => {
    expect(windBand(2.9)).toBe(WIND_SCALE[0]);
    expect(windBand(3)).toBe(WIND_SCALE[1]);
    expect(windBand(14.99)).toBe(WIND_SCALE[6]);
    expect(windBand(15)).toBe(WIND_SCALE[7]);
    expect(windBand(34.9)).toBe(WIND_SCALE[12]);
    expect(windBand(35)).toBe(WIND_SCALE[13]);
    expect(windBand(80)).toBe(WIND_SCALE[13]);
  });

  it('puts calm and invalid values in the first band', () => {
    expect(windBand(0)).toBe(WIND_SCALE[0]);
    expect(windBand(-1)).toBe(WIND_SCALE[0]);
    expect(windBand(Number.NaN)).toBe(WIND_SCALE[0]);
  });

  it('has ascending edges starting at 0', () => {
    expect(WIND_SCALE[0].from).toBe(0);
    for (let i = 1; i < WIND_SCALE.length; i++) {
      expect(WIND_SCALE[i].from).toBeGreaterThan(WIND_SCALE[i - 1].from);
    }
  });

  it('keeps every number on its colour readable', () => {
    for (const b of WIND_SCALE) expect(contrast(b.bg, b.fg), `${b.from} kn`).toBeGreaterThanOrEqual(4.5);
  });
});
