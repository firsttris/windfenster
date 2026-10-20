import { describe, expect, it } from 'vitest';
import {
  ago,
  clock,
  compass,
  coordinates,
  countryName,
  dateOf,
  dayMonth,
  dayMonthOf,
  formatNumber,
  km,
  runHour,
  weekday,
} from './format';

// 2026-10-11 08:50 UTC
const AT = 1_791_708_600;

describe('numbers, dates and places in German', () => {
  it('writes numbers and distances', () => {
    expect(formatNumber(1234.5, 1)).toBe('1.234,5');
    expect(formatNumber(2.25, 1)).toBe('2,3');
    expect(km(4.25)).toBe('4,3 km');
    expect(km(12.4)).toBe('12 km');
    expect(coordinates(48.7794, 8.0805)).toBe('48,78° N · 8,08° E');
    expect(coordinates(-33.9, -18.42)).toBe('33,90° S · 18,42° W');
  });

  it("writes days and times, in the place's time zone", () => {
    // "So." in browsers, "So" in Node: their ICU data differ
    expect(weekday('2026-10-11', 'short')).toMatch(/^So\.?$/);
    expect(weekday('2026-10-11', 'long')).toBe('Sonntag');
    expect(dayMonth('2026-10-11')).toBe('11. Okt.');
    expect(clock(AT, 'Europe/Berlin')).toBe('10:50');
    expect(clock(AT, 'Europe/Athens')).toBe('11:50');
    // Late in the UTC day it is already tomorrow in Athens
    expect(dayMonthOf(AT + 13 * 3_600, 'Europe/Athens')).toBe('12. Okt.');
    expect(dateOf(AT, 'Europe/Berlin')).toBe('11. Okt. 2026');
    expect(runHour(AT)).toBe('08 UTC');
  });

  it('says how long ago', () => {
    expect(ago(AT, AT + 20)).toBe('gerade eben');
    expect(ago(AT, AT + 38 * 60)).toBe('vor 38 Min.');
    expect(ago(AT, AT + 3 * 3_600)).toBe('vor 3 Std.');
    expect(ago(AT, AT + 3 * 86_400)).toBe('vor 3 Tagen');
  });

  it('names countries and compass points', () => {
    expect(countryName('DE')).toBe('Deutschland');
    expect(countryName('GR')).toBe('Griechenland');
    expect(countryName(null)).toBeNull();
    expect(countryName('not a code')).toBe('not a code');
    expect([0, 45, 90, 180, 225, 270, 350].map(compass)).toEqual(['N', 'NO', 'O', 'S', 'SW', 'W', 'N']);
  });
});
