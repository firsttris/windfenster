import { describe, expect, it } from 'vitest';
import type { MonthStats } from '@/bindings/MonthStats';
import type { WindStats } from '@/bindings/WindStats';
import { petal, rings, rose, sectorDegrees, wholeYear, windyShare } from './wind-stats';

const empty = (): number[][] => Array.from({ length: 16 }, () => Array(8).fill(0));

/** 100 hours: 60 from the west (40 at 13–17 kn, 20 at 17–21), 30 from the north at 5–9, 10 calm */
function month(windyDays = 20, days = 30): MonthStats {
  const r = empty();
  r[12][3] = 40;
  r[12][4] = 20;
  r[0][1] = 30;
  return { hours: 100, calm: 10, rose: r, days, windyDays };
}

describe('wind statistics as the page shows them', () => {
  it('reads a month', () => {
    const got = rose(month());
    expect(got.sectors[12][3]).toBeCloseTo(0.4);
    expect(got.max).toBeCloseTo(0.6);
    expect(got.calm).toBeCloseTo(0.1);
    expect(got.main).toBe(12);
    expect(sectorDegrees(got.main ?? 0)).toBe(270);
    expect(windyShare(month())).toBeCloseTo(2 / 3);
    expect(windyShare(month(0, 0))).toBeNull();
    expect(rose({ ...month(), rose: empty(), calm: 100 }).main).toBeNull();
  });

  it('adds the months up to the year', () => {
    const stats: WindStats = {
      firstYear: 2016,
      lastYear: 2025,
      source: 'iem',
      months: Array.from({ length: 12 }, () => month()),
    };
    const year = wholeYear(stats);
    expect([year.hours, year.calm, year.days, year.windyDays, year.rose[12][3]]).toEqual([1200, 120, 360, 240, 480]);
  });

  it('stacks a petal outwards, weakest inside, and rings up to the fullest', () => {
    const r = rose(month());
    expect(petal(r.sectors[12], r.max)).toEqual([
      { band: 3, from: 0, to: expect.closeTo(0.667, 3) },
      { band: 4, from: expect.closeTo(0.667, 3), to: expect.closeTo(1, 6) },
    ]);
    expect(petal(r.sectors[5], r.max)).toEqual([]);
    expect(rings(0.6)).toEqual([0.1, 0.2, 0.3, 0.4, 0.5, 0.6]);
    expect(rings(0.13)).toEqual([0.05, 0.1, 0.15]);
    expect(rings(0.03)).toEqual([0.01, 0.02, 0.03]);
    expect(rings(0.031)).toEqual([0.01, 0.02, 0.03, 0.04]);
    expect(rings(0)).toEqual([0.01]);
  });
});
