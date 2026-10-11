import { describe, expect, it } from 'vitest';
import { spark } from './spark';

const o = (at: number, wind: number | null, gust: number | null = null) => ({
  at,
  wind,
  gust,
  dir: null,
  temp: null,
  raw: null,
});

describe('the sparkline', () => {
  it('draws the wind on a scale to the strongest value', () => {
    const s = spark([o(0, 0), o(50, 10, 20), o(100, 20)], 0, 100, 100, 22);
    expect(s?.max).toBe(20);
    expect(s?.line).toBe('M0.0,21.0L50.0,11.0L100.0,1.0');
    expect(s?.area).toBe('M0.0,21.0L50.0,11.0L100.0,1.0L100.0,22L0.0,22Z');
    expect(s?.gusts).toEqual([{ x: 50, y: 1 }]);
  });

  it('keeps calm low and needs two measurements', () => {
    expect(spark([o(0, 2), o(10, 3)], 0, 10, 10, 12)?.max).toBe(10);
    expect(spark([o(0, 2), o(10, null)], 0, 10, 10, 12)).toBeNull();
  });
});
