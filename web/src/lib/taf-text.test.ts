import { describe, expect, it } from 'vitest';
import type { TafGroup } from '@/bindings/TafGroup';
import { groupsBetween, groupWhat, groupWhen, prevailingBefore } from './taf-text';

// TAF EDSB 110500Z 1106/1206 22007KT CAVOK BECMG 1117/1120 VRB03KT, as aviationweather.gov
// decodes it (2026-10-11); station time Europe/Berlin.
const T0 = 1_791_698_400; // 06 UTC = 08 local
const H = 3_600;
const base: TafGroup = {
  change: 'base',
  probability: null,
  from: T0,
  to: T0 + 11 * H,
  by: null,
  dir: 220,
  variable: false,
  wind: 7,
  gust: null,
  visibilityM: 9999,
  weather: 'NSW',
  clouds: [{ cover: 'NSC', baseFt: null, kind: null }],
};
const becmg: TafGroup = {
  ...base,
  change: 'becoming',
  from: T0 + 11 * H,
  to: T0 + 24 * H,
  by: T0 + 14 * H,
  dir: null,
  variable: true,
  wind: 3,
};
const ctx = { unit: 'kn' as const, at: T0, timeZone: 'Europe/Berlin' };

describe('TAF groups in words', () => {
  it('say when, in station time', () => {
    expect(groupWhen(base, T0, 'Europe/Berlin')).toBe('bis 19:00');
    expect(groupWhen(becmg, T0, 'Europe/Berlin')).toBe('ab 19–22');
    const tomorrow = { ...base, change: 'tempo' as const, probability: 30, from: T0 + 22 * H, to: T0 + 24 * H };
    // ICU versions differ in the short weekday ("Mo" or "Mo.").
    expect(groupWhen(tomorrow, T0, 'Europe/Berlin')).toMatch(/^30 % zeitw\. Mo\.? 06–08$/);
  });

  it('say what: wind, visibility, weather, low clouds', () => {
    expect(groupWhat(base, ctx)).toBe('SW (220°), 7 kn, Sicht 10 km oder mehr');
    // Visibility and NSW are carried over from the base: not told again.
    expect(groupWhat(becmg, ctx, base)).toBe('umlaufend, 3 kn');
    expect(prevailingBefore([base, becmg])).toEqual([null, base]);
    const showers: TafGroup = {
      ...base,
      change: 'tempo',
      wind: null,
      dir: null,
      visibilityM: 5000,
      weather: 'RA',
      clouds: [
        { cover: 'FEW', baseFt: 1800, kind: 'TCU' },
        { cover: 'BKN', baseFt: 2000, kind: null },
      ],
    };
    expect(groupWhat(showers, ctx, base)).toBe(
      'Sicht 5 km, Regen, wenige Wolken in 1.800 ft, hoch aufquellend, durchbrochen in 2.000 ft',
    );
    // Scattered clouds only when nothing else changes.
    const sct: TafGroup = {
      ...showers,
      visibilityM: null,
      weather: null,
      clouds: [{ cover: 'SCT', baseFt: 1000, kind: null }],
    };
    expect(groupWhat(sct, ctx, base)).toBe('aufgelockert in 1.000 ft');
    // After rain, NSW says it stops.
    const dry: TafGroup = { ...base, change: 'becoming', wind: null, weather: 'NSW', clouds: [] };
    expect(groupWhat(dry, ctx, { ...base, weather: 'RA' })).toBe('kein nennenswertes Wetter');
  });

  it('pick the change groups that touch a day, each with the group it changes', () => {
    // The day the BECMG starts in: the BECMG, after the base
    const hit = groupsBetween([base, becmg], T0 + 10 * H, T0 + 13 * H);
    expect(hit).toEqual([{ group: becmg, prev: base }]);
    // Before it nothing changes: the base alone is in the numbers already
    expect(groupsBetween([base, becmg], T0, T0 + 11 * H)).toEqual([]);
  });
});
