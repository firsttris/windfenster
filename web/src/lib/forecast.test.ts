import { describe, expect, it } from 'vitest';
import type { Forecast } from '@/bindings/Forecast';
import type { Model } from '@/bindings/Model';
import type { ModelSeries } from '@/bindings/ModelSeries';
import type { Ranking } from '@/bindings/Ranking';
import {
  autoModel,
  barShare,
  buildDays,
  compassIndex,
  defaultModel,
  hoursOn,
  initialModel,
  isNight,
  localClock,
  modeOf,
  weatherOf,
} from './forecast';

const MIDNIGHT = 1_791_669_600; // 2026-10-11 00:00 in Berlin (22:00 UTC the day before)
const HOUR = 3600;

const model = (id: string, cls: 'super' | 'global' | 'mix', rank: number): Model => ({
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
  it('lays out ten whole days in three stages', () => {
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
    // Every 3 hours around the clock, the trend days as full
    expect(days[2].cells.map((c) => c.hour)).toEqual([2, 5, 8, 11, 14, 17, 20, 23]);
    expect(days[9].cells.map((c) => c.hour)).toEqual([2, 5, 8, 11, 14, 17, 20, 23]);
  });

  it('lays out today and tomorrow every 3 hours too when not asked hourly', () => {
    const days = buildDays(fc, 'icon_d2', MIDNIGHT + 10 * HOUR, undefined, false);
    expect(days.slice(0, 3).map((d) => d.stage)).toEqual(['threeHourly', 'threeHourly', 'threeHourly']);
    expect(days[0].cells.map((c) => c.hour)).toEqual([2, 5, 8, 11, 14, 17, 20, 23]);
    expect(days[0].cells.filter((c) => c.past).map((c) => c.hour)).toEqual([2, 5]);
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
    expect(days[8].cells[0].rain).toBe(0.3);
  });

  it('marks hours that are over', () => {
    const cells = buildDays(fc, 'icon_d2', MIDNIGHT + 10 * HOUR)[0].cells;
    expect(cells.filter((c) => c.past).map((c) => c.hour)).toEqual([7, 8, 9]);
  });

  it("keeps the day of the spring clock change, which has no 2 o'clock", () => {
    // ICON-D2 from 27 March 2026 00 UTC for 48 hours, ECMWF for ten days: the 29th, when the
    // clocks go forward, lies past ICON-D2 and is ECMWF's
    const start = Date.UTC(2026, 2, 27) / 1000;
    const run = (id: string, hours: number): ModelSeries => ({
      ...series(id, hours, 0),
      runAt: start,
      fetchedAt: start,
      start,
    });
    const spring = { ...fc, series: [run('icon_d2', 48), run('ecmwf', 240)] };
    const day = buildDays(spring, 'icon_d2', start + 10 * HOUR).find((d) => d.date === '2026-03-29');
    expect(day?.source).toBe('ecmwf');
    expect(day?.cells.map((c) => c.hour)).toEqual([5, 8, 11, 14, 17, 20, 23]);
    expect(day?.cells.every((c) => c.wind !== null)).toBe(true);
  });

  it("knows the hours of a day in the place's zone", () => {
    const berlin = localClock('Europe/Berlin');
    expect(hoursOn('2026-03-29', berlin).has(2)).toBe(false);
    expect(hoursOn('2026-03-29', berlin).size).toBe(23);
    expect(hoursOn('2026-10-25', berlin).size).toBe(24);
    expect(hoursOn('2026-10-12', localClock('Pacific/Kiritimati')).size).toBe(24);
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

describe('a fine model that ends within a day', () => {
  // ICON-CH1 reaches 33 hours: at 11:00 in Zurich its 06 UTC run stops at 17:00 tomorrow
  const run06 = MIDNIGHT + 8 * HOUR;
  const ch1Hours = (run06 + 33 * HOUR - MIDNIGHT) / HOUR + 1;
  const flat = (id: string, hours: number, wind: number): ModelSeries => ({
    ...series(id, hours, 0),
    wind: Array.from({ length: hours }, () => wind),
  });
  const zurich: Forecast = {
    ...fc,
    timezone: 'Europe/Zurich',
    models: [
      { ...model('meteoswiss_icon_ch1', 'super', 12), maxLeadH: 33 },
      { ...model('meteoswiss_icon_ch2', 'super', 25), maxLeadH: 120 },
      model('ecmwf_ifs025', 'global', 40),
    ],
    series: [
      flat('meteoswiss_icon_ch1', ch1Hours, 18),
      flat('meteoswiss_icon_ch2', 24 * 6, 17),
      flat('ecmwf_ifs025', 24 * 10, 9),
    ],
  };

  it('hands tomorrow to the next fine model that has it, not the global one', () => {
    const days = buildDays(zurich, 'meteoswiss_icon_ch1', MIDNIGHT + 11 * HOUR, 'ecmwf_ifs025');
    expect(days[0].source).toBe('meteoswiss_icon_ch1');
    expect(days[1].stage).toBe('hourly');
    expect(days[1].source).toBe('meteoswiss_icon_ch2');
    expect(days[1].cells.every((c) => c.wind === 17)).toBe(true);
    // From day 3 the global model, as before
    expect(days[2].source).toBe('ecmwf_ifs025');
  });

  it('leaves a chosen global model to the global fallback', () => {
    const days = buildDays(zurich, 'ecmwf_ifs025', MIDNIGHT + 11 * HOUR, 'ecmwf_ifs025', false);
    expect(days.every((d) => d.source === 'ecmwf_ifs025')).toBe(true);
  });
});

describe('model choice', () => {
  const ranking = (r: Partial<Ranking>): Ranking => ({
    days: 20,
    scores: [],
    bestSuper: null,
    bestGlobal: null,
    mixFirst: false,
    ...r,
  });
  const withMix: Forecast = {
    ...fc,
    models: [model('mix', 'mix', 0), ...fc.models],
    series: [series('mix', 240, 0), ...fc.series],
  };

  it('follows the default order without a ranking', () => {
    expect(initialModel(withMix, null)).toBe('icon_d2');
    expect(autoModel(withMix, null, 'global')).toBe('ecmwf');
    expect(modeOf(withMix, 'mix')).toBeNull();
    expect(modeOf(withMix, 'gfs')).toBe('global');
  });

  it('fills the ten days with a global model that reaches them', () => {
    // ICON-EU: global class, 5 days, ranked before GFS
    const short = { ...model('icon_eu', 'global', 45), maxLeadH: 120 };
    const eu: Forecast = {
      ...fc,
      models: [fc.models[0], short, fc.models[2]],
      series: [fc.series[0], series('icon_eu', 120, 0), fc.series[2]],
    };
    expect(autoModel(eu, null, 'global')).toBe('gfs');
    expect(buildDays(eu, 'icon_eu', MIDNIGHT + 10 * HOUR, autoModel(eu, null, 'global'))).toHaveLength(10);
    // Alone it still opens
    const only: Forecast = { ...fc, models: [short], series: [series('icon_eu', 120, 0)] };
    expect(autoModel(only, null, 'global')).toBe('icon_eu');
  });

  it('takes what the ranking picked', () => {
    expect(autoModel(withMix, ranking({ bestGlobal: 'gfs' }), 'global')).toBe('gfs');
    expect(initialModel(withMix, ranking({ mixFirst: true }))).toBe('mix');
    // A pick without values here falls back to the order.
    expect(autoModel(withMix, ranking({ bestSuper: 'arome' }), 'super')).toBe('icon_d2');
  });

  it('lets the chosen global model take over after the regional one', () => {
    const days = buildDays(withMix, 'icon_d2', MIDNIGHT + 10 * HOUR, 'gfs');
    expect(days[2].source).toBe('gfs');
  });
});

describe('bar heights and night hours', () => {
  it('grows faster in light wind and is full from 28 kn', () => {
    expect(barShare(0)).toBe(0);
    expect(barShare(-3)).toBe(0);
    expect(barShare(28)).toBe(1);
    expect(barShare(40)).toBe(1);
    // 7 kn stands at 44 % rather than a quarter
    expect(barShare(7)).toBeCloseTo(0.435, 2);
    expect(barShare(14)).toBeGreaterThan(barShare(7));
  });

  it('takes the hours before 7 and after 20 for night', () => {
    expect([2, 5, 8, 11, 14, 17, 20, 23].filter(isNight)).toEqual([2, 5, 23]);
  });
});
