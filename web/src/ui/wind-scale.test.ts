import { describe, expect, it } from 'vitest';
import { WIND_SCALE, windBand } from './wind-scale';

describe('windBand', () => {
  it('uses inclusive lower edges', () => {
    expect(windBand(4.9)).toBe(WIND_SCALE[0]);
    expect(windBand(5)).toBe(WIND_SCALE[1]);
    expect(windBand(13.99)).toBe(WIND_SCALE[3]);
    expect(windBand(14)).toBe(WIND_SCALE[3]);
    expect(windBand(29.9)).toBe(WIND_SCALE[6]);
    expect(windBand(30)).toBe(WIND_SCALE[7]);
    expect(windBand(80)).toBe(WIND_SCALE[7]);
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
});
