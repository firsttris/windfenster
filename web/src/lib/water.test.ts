import { describe, expect, it } from 'vitest';
import { levelTrend } from './water';

const H = 3600;
const series = (cms: number[]) => cms.map((cm, i) => ({ at: i * H, cm }));

describe('levelTrend', () => {
  it('compares with a day earlier, the value it names', () => {
    // Down from 73 cm a day earlier, with a short rise at the end, as at a weir
    const day = series([...Array(25).fill(73), 58, 57, 59, 60]);
    expect(levelTrend(day)).toEqual({ trend: 'falling', yesterday: 73 });
    expect(levelTrend(series([...Array(25).fill(60), 62])).trend).toBe('steady');
  });

  it('falls back to three hours without a day of history', () => {
    expect(levelTrend(series([60, 60, 61, 61])).trend).toBe('steady');
    expect(levelTrend(series([60, 62, 64, 66]))).toEqual({ trend: 'rising', yesterday: null });
  });

  it('says nothing without enough history', () => {
    expect(levelTrend([])).toEqual({ trend: null, yesterday: null });
    expect(levelTrend(series([60]))).toEqual({ trend: null, yesterday: null });
  });
});
