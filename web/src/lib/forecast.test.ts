import { describe, expect, it } from 'vitest';
import type { Forecast } from '@/bindings/Forecast';
import type { Model } from '@/bindings/Model';
import type { ModelSeries } from '@/bindings/ModelSeries';
import { buildDays, compassIndex, defaultModel, localClock, modeOf, weatherOf } from './forecast';

const MIDNIGHT = 1_791_669_600; // 2026-10-11 00:00 in Berlin (22:00 UTC the day before)
const HOUR = 3600;

const model = (id: string, cls: 'super' | 'global', rank: number): Model => ({
  id,
  name: id,
  provider: 'test',
  resolutionKm: cls === 'super' ? 2 : 25,
  class: cls,
  domain: null,
  maxLeadH: cls === 'super' ? 48 : 360,
  runsPerDay: 4,
  rank,
});

const series = (id: string, hours: number, rain: number): ModelSeries => ({
  model: id,
  runAt: MIDNIGHT,
  fetchedAt: MIDNIGHT,
  start: MIDNIGHT,
  step: HOUR,
  wind: Array.from({ length: hours }, (_, i) => i % 24),
  gust: Array.from({ length: hours }, (_, i) => (i % 24) + 5),
  dir: Array.from({ length: hours }, () => 225),
  temp: Array.from({ length: hours }, () => 15),
  rain: Array.from({ length: hours }, () => rain),
  cloud: Array.from({ length: hours }, () => 50),
});

const fc: Forecast = {
  lat: 48.78,
  lon: 8.08,
  timezone: 'Europe/Berlin',
  elevationM: 120,
  models: [model('icon_d2', 'super', 10), model('ecmwf', 'global', 40), model('gfs', 'global', 50)],
  series: [series('icon_d2', 48, 0), series('ecmwf', 240, 0.1), series('gfs', 240, 0)],
  notice: null,
};

describe('buildDays', () => {
  it('lays out ten days in three stages', () => {
    const days = buildDays(fc, 'icon_d2', MIDNIGHT + 10 * HOUR);
    expect(days).toHaveLength(10);
    expect(days.map((d) => d.stage)).toEqual([
      'hourly',
      'hourly',
      'threeHourly',
      'threeHourly',
      'threeHourly',
      'threeHourly',
      'threeHourly',
      'trend',
      'trend',
      'trend',
    ]);
    expect(days[0].date).toBe('2026-10-11');
    expect(days[0].cells.map((c) => c.hour)).toEqual([7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19]);
    expect(days[2].cells.map((c) => c.hour)).toEqual([6, 9, 12, 15, 18, 21]);
    expect(days[9].cells.map((c) => c.hour)).toEqual([6, 12, 18]);
  });

  it('takes the chosen model while it lasts, then the best global one', () => {
    const days = buildDays(fc, 'icon_d2', MIDNIGHT);
    expect(days.map((d) => d.source).slice(0, 3)).toEqual(['icon_d2', 'icon_d2', 'ecmwf']);
    expect(days[0].cells[0]).toMatchObject({ hour: 7, wind: 7, gust: 12, dir: 225, temp: 15 });
    expect(buildDays(fc, 'gfs', MIDNIGHT)[0].source).toBe('gfs');
  });

  it('sums rain over the hours a cell stands for', () => {
    const days = buildDays(fc, 'ecmwf', MIDNIGHT);
    expect(days[0].cells[0].rain).toBe(0.1);
    expect(days[3].cells[0].rain).toBe(0.3);
    expect(days[8].cells[0].rain).toBe(0.6);
  });

  it('marks hours that are over', () => {
    const cells = buildDays(fc, 'icon_d2', MIDNIGHT + 10 * HOUR)[0].cells;
    expect(cells.filter((c) => c.past).map((c) => c.hour)).toEqual([7, 8, 9]);
  });

  it('is empty without series', () => {
    expect(buildDays({ ...fc, series: [] }, 'icon_d2', MIDNIGHT)).toEqual([]);
  });
});

describe('models', () => {
  it('picks the first model of a class that has values', () => {
    expect(defaultModel(fc, 'super')).toBe('icon_d2');
    expect(defaultModel(fc, 'global')).toBe('ecmwf');
    expect(defaultModel({ ...fc, series: fc.series.slice(1) }, 'super')).toBeNull();
    expect(modeOf(fc, 'gfs')).toBe('global');
  });
});

describe('helpers', () => {
  it('reads local time in the place’s zone', () => {
    expect(localClock('Europe/Berlin')(MIDNIGHT + 7 * HOUR)).toEqual({ date: '2026-10-11', hour: 7 });
    expect(localClock('Europe/Athens')(MIDNIGHT)).toEqual({ date: '2026-10-11', hour: 1 });
  });

  it('turns clouds and rain into a symbol', () => {
    expect(weatherOf(10, 0)).toBe('sun');
    expect(weatherOf(40, 0)).toBe('partly');
    expect(weatherOf(90, 0.1)).toBe('cloud');
    expect(weatherOf(90, 1.2)).toBe('rain');
    expect(weatherOf(null, null)).toBeNull();
  });

  it('names compass points', () => {
    expect(compassIndex(0)).toBe(0);
    expect(compassIndex(225)).toBe(10);
    expect(compassIndex(350)).toBe(0);
    expect(compassIndex(-90)).toBe(12);
  });
});
