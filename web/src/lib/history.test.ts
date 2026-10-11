import { describe, expect, it } from 'vitest';
import type { Observation } from '@/bindings/Observation';
import { arrowPicks, dayOptions, dayStats, isolated, localMidnight, TIP, tipPlacement } from './history';

const o = (at: number, wind: number | null, dir: number | null, gust: number | null = null): Observation => ({
  at,
  wind,
  gust,
  dir,
  temp: null,
  raw: null,
});

describe('days', () => {
  it('finds local midnight, also across the clock change', () => {
    expect(localMidnight('2026-10-11', 'Europe/Berlin')).toBe(1_791_669_600);
    // 2026-10-25: summer time ends, the day has 25 hours
    expect(localMidnight('2026-10-26', 'Europe/Berlin') - localMidnight('2026-10-25', 'Europe/Berlin')).toBe(25 * 3600);
    expect(localMidnight('2026-10-11', 'Europe/Athens')).toBe(1_791_669_600 - 3600);
  });

  it('offers the last 24 hours and ten days before', () => {
    const now = 1_791_669_600 + 12 * 3600; // 2026-10-11 12:00 Berlin
    const days = dayOptions(now, 'Europe/Berlin');
    expect(days).toHaveLength(11);
    expect(days[0]).toEqual({ key: 'today', from: now - 86_400, to: now, date: '2026-10-11' });
    expect(days[1]).toEqual({ key: '2026-10-10', from: 1_791_669_600 - 86_400, to: 1_791_669_600, date: '2026-10-10' });
    expect(days[10].date).toBe('2026-10-01');
  });
});

describe('dayStats', () => {
  it('sums up a day', () => {
    const s = dayStats([o(0, 10, 350, 15), o(1, 20, 10, 28), o(2, null, null, null)]);
    expect([s.maxWind, s.maxGust, s.meanWind]).toEqual([20, 28, 15]);
    // Pulled towards the stronger wind from 10°
    expect(s.mainDir).toBeGreaterThan(0);
    expect(s.mainDir).toBeLessThan(10);
    expect(dayStats([])).toEqual({ maxWind: null, maxGust: null, meanWind: null, mainDir: null });
  });

  it('spreads the direction arrows', () => {
    const obs = Array.from({ length: 144 }, (_, i) => o(i * 600, 10, 230));
    // 24 h on 336 px with 28 px apart: one per 2 hours
    expect(arrowPicks(obs, 0, 86_400, 336)).toHaveLength(12);
    // Never more than one an hour
    expect(arrowPicks(obs, 0, 86_400, 4000)).toHaveLength(24);
    expect(arrowPicks([o(0, 0, null)], 0, 86_400, 300)).toHaveLength(0);
  });
});

describe('tipPlacement', () => {
  it('sits centred above the point', () => {
    expect(tipPlacement(300, 150, 600)).toEqual({ left: 300 - TIP.width / 2, top: 150 - TIP.gap - TIP.height });
  });

  it('stays inside the chart at its edges', () => {
    expect(tipPlacement(10, 150, 600).left).toBe(0);
    expect(tipPlacement(595, 150, 600).left).toBe(600 - TIP.width);
  });

  it('goes below the point when the top is too close', () => {
    expect(tipPlacement(300, 40, 600).top).toBe(40 + TIP.gap);
  });
});

describe('isolated', () => {
  it('finds values with gaps on both sides', () => {
    expect(isolated([20, null, 22, 23, null, 25])).toEqual([0, 5]);
    expect(isolated([null, 18, null])).toEqual([1]);
    expect(isolated([1, 2])).toEqual([]);
    expect(isolated([])).toEqual([]);
  });
});
