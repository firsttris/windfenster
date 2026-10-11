import { describe, expect, it } from 'vitest';
import { formatWind } from './units';

describe('formatWind', () => {
  it('rounds knots and km/h to whole numbers', () => {
    expect(formatWind(14.4, 'kn')).toBe('14');
    expect(formatWind(14.5, 'kn')).toBe('15');
    expect(formatWind(10, 'kmh')).toBe('19');
  });

  it('shows m/s with one decimal and a comma', () => {
    expect(formatWind(10, 'ms')).toBe('5,1');
    expect(formatWind(0, 'ms')).toBe('0,0');
  });
});
