import { describe, expect, it } from 'vitest';
import { dayGlance, mainDirection, spotWorks } from './day-glance';
import type { Cell, Day } from './forecast';

const cell = (hour: number, wind: number | null, dir: number | null, gust: number | null = null): Cell => ({
  time: hour * 3600,
  hour,
  wind,
  gust,
  dir,
  temp: 15,
  rain: 0,
  cloud: 0,
  past: false,
});

describe('a day at a glance', () => {
  it('takes the main direction from the wind vectors, strong hours weighing more', () => {
    // 10 kn from north-east and 2 kn from south: north-east wins, pulled a little toward east
    expect(mainDirection([cell(11, 10, 45), cell(14, 2, 180)])).toBeCloseTo(54, 0);
    // Across north: 350° and 10° give north, not south
    const n = mainDirection([cell(11, 8, 350), cell(14, 8, 10)]) ?? -1;
    expect(Math.min(n, 360 - n)).toBeLessThan(0.01);
    expect(mainDirection([cell(11, 0, 90), cell(14, null, null)])).toBeNull();
  });

  it('gives the daytime strongest wind and main direction, the night left out', () => {
    const day: Day = {
      date: '2026-10-18',
      stage: 'threeHourly',
      source: 'ecmwf',
      cells: [cell(2, 20, 180, 25), cell(11, 9, 30, 12), cell(14, 10, 20), cell(23, 3, 200)],
    };
    const g = dayGlance(day);
    expect(g.max).toBe(10);
    expect(g.main).toBeGreaterThan(20);
    expect(g.main).toBeLessThan(30);
    expect(g.bars.map((b) => [b.hour, b.night, b.gust])).toEqual([
      [2, true, 25],
      [11, false, 12],
      [14, false, 10],
      [23, true, 3],
    ]);
    expect(g.works).toBe(false);
  });

  it('marks the hours that work at the spot: daylight, its directions, enough wind', () => {
    const works = spotWorks({ from: 0, to: 90 }, []);
    expect(works(cell(14, 12, 45))).toBe(true);
    expect(works(cell(14, 11, 45))).toBe(false);
    expect(works(cell(14, 15, 200))).toBe(false);
    expect(works(cell(23, 15, 45))).toBe(false);
    // Any direction without a sector
    expect(spotWorks(null, [])(cell(14, 12, 200))).toBe(true);
    // With gear: what fits it, not the 12 kn
    const gear = [{ kind: 'kite', size: '12', minKn: 15, maxKn: 22 }] as never;
    expect(spotWorks(null, gear)(cell(14, 14, 45))).toBe(false);
    expect(spotWorks(null, gear)(cell(14, 18, 45))).toBe(true);
    const day: Day = { date: 'd', stage: 'threeHourly', source: 'x', cells: [cell(11, 13, 30), cell(14, 5, 30)] };
    expect(dayGlance(day, works).bars.map((b) => b.works)).toEqual([true, false]);
    expect(dayGlance(day, works).works).toBe(true);
  });
});
