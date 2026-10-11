import { describe, expect, it } from 'vitest';
import type { Forecast } from '@/bindings/Forecast';
import { compareData } from './compare';

const MIDNIGHT = 1_791_669_600; // 2026-10-11 00:00 in Berlin
const H = 3600;

const fc: Forecast = {
  lat: 48.78,
  lon: 8.08,
  timezone: 'Europe/Berlin',
  elevationM: null,
  models: [],
  series: [
    {
      model: 'icon_d2',
      runAt: MIDNIGHT,
      fetchedAt: MIDNIGHT,
      start: MIDNIGHT,
      step: H,
      wind: Array.from({ length: 48 }, (_, h) => h),
      gust: [],
      dir: [],
      temp: [],
      rain: [],
      cloud: [],
    },
  ],
  notice: null,
};

describe('compareData', () => {
  it('takes today from 7 to 19 with the measurements so far', () => {
    const obs = [
      { at: MIDNIGHT + 6 * H, wind: 3, gust: null, dir: null, temp: null, raw: null },
      { at: MIDNIGHT + 10 * H + 1200, wind: 9, gust: null, dir: null, temp: null, raw: null },
    ];
    const d = compareData(fc, obs, MIDNIGHT + 11 * H, 'kn');
    expect(d?.from).toBe(MIDNIGHT + 7 * H);
    expect(d?.to).toBe(MIDNIGHT + 19 * H);
    // 13 hours and one measurement in between (the one at 6 is before 7)
    expect(d?.x).toHaveLength(14);
    const i = d?.x.indexOf(MIDNIGHT + 10 * H + 1200) ?? -1;
    expect(d?.measured[i]).toBe(9);
    expect(d?.lines[0][i]).toBeNull();
    expect(d?.lines[0][0]).toBe(7);
    expect(compareData(fc, obs, MIDNIGHT + 11 * H, 'kmh')?.measured[i]).toBe(16.7);
  });
});
