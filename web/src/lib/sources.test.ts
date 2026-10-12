import { describe, expect, it } from 'vitest';
import { DATA_SOURCES, SOFTWARE } from './sources';

describe('the sources page', () => {
  const all = [...DATA_SOURCES, ...SOFTWARE];

  it('names every source once, with a link, its use and its licence', () => {
    expect(new Set(all.map((s) => s.id)).size).toBe(all.length);
    for (const s of all) {
      expect(s.url, s.id).toMatch(/^https:\/\//);
      expect(s.use().length, s.id).toBeGreaterThan(5);
      const licence = typeof s.licence.name === 'function' ? s.licence.name() : s.licence.name;
      expect(licence.length, s.id).toBeGreaterThan(2);
      expect(s.licence.url, s.id).toMatch(/^(https:\/\/|\/)/);
    }
  });

  it('has every source the app shows data from', () => {
    const ids = DATA_SOURCES.map((s) => s.id);
    for (const id of ['open-meteo', 'dwd', 'ecmwf', 'awc', 'pegelonline', 'ourairports', 'osm', 'mapterhorn'])
      expect(ids).toContain(id);
  });
});
