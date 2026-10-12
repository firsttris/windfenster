import { describe, expect, it } from 'vitest';
import type { Forecast } from '@/bindings/Forecast';
import type { ModelSeries } from '@/bindings/ModelSeries';
import { bestTime, dayAgreement, levelOf, spreadAt, stripTop } from './agreement';

const MIDNIGHT = 1_791_669_600; // 2026-10-11 00:00 in Berlin
const H = 3600;

const series = (model: string, wind: (h: number) => number | null): ModelSeries => ({
  model,
  runAt: MIDNIGHT,
  fetchedAt: MIDNIGHT,
  start: MIDNIGHT,
  step: H,
  wind: Array.from({ length: 72 }, (_, h) => wind(h)),
  gust: [],
  dir: [],
  temp: [],
  rain: [],
  cloud: [],
});

const fc = (s: ModelSeries[]): Forecast => ({
  lat: 48.78,
  lon: 8.08,
  timezone: 'Europe/Berlin',
  elevationM: null,
  models: [],
  series: s,
  notice: null,
});

describe('agreement', () => {
  it('grades the spread', () => {
    expect([levelOf(4), levelOf(5), levelOf(8), levelOf(9)]).toEqual(['high', 'medium', 'medium', 'low']);
  });

  it('leaves the mix out of the spread', () => {
    const f = fc([series('mix', () => 30), series('a', () => 10), series('b', () => 16)]);
    expect(spreadAt(f, MIDNIGHT + 5 * H)).toMatchObject({ spread: 6, level: 'medium' });
    expect(spreadAt(fc([series('a', () => 10)]), MIDNIGHT)).toBeNull();
  });

  it('looks at the windiest daytime hour ahead', () => {
    // Windiest at 15 local, a storm at 23 local does not count (night)
    const mix = series('mix', (h) => (h === 15 ? 20 : h === 23 ? 40 : 10));
    const f = fc([mix, series('a', () => 10), series('b', (h) => (h === 15 ? 25 : 10))]);
    expect(bestTime(f, MIDNIGHT + 8 * H)).toBe(MIDNIGHT + 15 * H);
    expect(bestTime(f, MIDNIGHT + 16 * H)).not.toBe(MIDNIGHT + 15 * H);
    const day = dayAgreement(
      f,
      [9, 12, 15, 18].map((h) => MIDNIGHT + h * H),
    );
    expect(day).toMatchObject({ time: MIDNIGHT + 15 * H, spread: 15, level: 'low' });
  });
});

describe('the agreement strip', () => {
  it('reaches 30 kn, further when a model says more', () => {
    expect(stripTop([8, 12])).toBe(30);
    expect(stripTop([30])).toBe(30);
    expect(stripTop([36, 44, 46])).toBe(50);
    expect(stripTop([])).toBe(30);
  });
});
