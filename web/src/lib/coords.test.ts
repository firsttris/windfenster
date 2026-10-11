import { describe, expect, it } from 'vitest';
import { parseCoordinates } from './coords';

describe('parseCoordinates', () => {
  it('reads the usual ways of writing a point', () => {
    expect(parseCoordinates('48.78, 8.08')).toEqual({ lat: 48.78, lon: 8.08 });
    expect(parseCoordinates('48,78, 8,08')).toEqual({ lat: 48.78, lon: 8.08 });
    expect(parseCoordinates('48,78 8,08')).toEqual({ lat: 48.78, lon: 8.08 });
    expect(parseCoordinates('48.78;8.08')).toEqual({ lat: 48.78, lon: 8.08 });
    expect(parseCoordinates(' -33.9 151.2 ')).toEqual({ lat: -33.9, lon: 151.2 });
    expect(parseCoordinates('36.4° 28.1°')).toEqual({ lat: 36.4, lon: 28.1 });
  });

  it('ignores names and values out of range', () => {
    expect(parseCoordinates('Baden Airpark')).toBeNull();
    expect(parseCoordinates('EDSB')).toBeNull();
    expect(parseCoordinates('95, 8')).toBeNull();
    expect(parseCoordinates('48')).toBeNull();
  });
});
