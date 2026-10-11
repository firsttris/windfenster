import { describe, expect, it } from 'vitest';
import type { Observation } from '@/bindings/Observation';
import { latest, nearest } from './observations';

const o = (at: number, wind: number | null = 10): Observation => ({
  at,
  wind,
  gust: null,
  dir: 210,
  temp: null,
  raw: null,
});

describe('nearest', () => {
  // METARs at :20 and :50
  const metars = [o(1_000 - 2_400), o(1_000 - 600), o(1_000 + 1_200)];

  it('takes the closest measurement within 20 minutes', () => {
    expect(nearest(metars, 1_000)?.at).toBe(400);
    expect(nearest(metars, 1_000 + 1_800)?.at).toBe(2_200);
  });

  it('takes none when the closest is farther away', () => {
    expect(nearest(metars, 1_000 - 6_000)).toBeNull();
    expect(nearest([], 1_000)).toBeNull();
  });
});

describe('latest', () => {
  it('skips measurements without wind', () => {
    expect(latest([o(1), o(2), o(3, null)])?.at).toBe(2);
    expect(latest([o(3, null)])).toBeNull();
  });
});
