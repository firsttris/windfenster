import { describe, expect, it } from 'vitest';
import type { Taf } from '@/bindings/Taf';
import type { TafHour } from '@/bindings/TafHour';
import { tafSegments } from './taf-segments';

const H = 3_600;
const hour = (at: number, wind: number | null, low: number | null = null, high: number | null = null): TafHour => ({
  at,
  wind,
  gust: null,
  dir: null,
  low,
  high,
  gustHigh: null,
  probability: null,
});
const taf = (hours: TafHour[]) => ({ hours }) as unknown as Taf;
const cells = (n: number, past = 0) => Array.from({ length: n }, (_, i) => ({ time: i * H, past: i < past }));

describe('the TAF along a day', () => {
  it('writes a value once for the hours in a row that share it', () => {
    const t = taf([hour(0, 3), hour(H, 3), hour(2 * H, 3), hour(3 * H, 6, 4, 8), hour(4 * H, 6, 4, 8), hour(5 * H, 6)]);
    expect(tafSegments(cells(6), t, 'kn')).toEqual([
      { from: 0, span: 3, text: '3', high: 3 },
      { from: 3, span: 2, text: '4–8', high: 8 },
      { from: 5, span: 1, text: '6', high: 6 },
    ]);
  });

  it('leaves hours that are over and hours without a TAF empty', () => {
    const t = taf([hour(0, 3), hour(H, 3), hour(2 * H, 3), hour(3 * H, null)]);
    expect(tafSegments(cells(5, 2), t, 'kn')).toEqual([
      { from: 0, span: 2, text: null, high: 0 },
      { from: 2, span: 1, text: '3', high: 3 },
      { from: 3, span: 2, text: null, high: 0 },
    ]);
  });

  it('is empty all along without a TAF', () => {
    expect(tafSegments(cells(3), null, 'kn')).toEqual([{ from: 0, span: 3, text: null, high: 0 }]);
  });

  it('speaks the chosen unit', () => {
    expect(tafSegments(cells(1), taf([hour(0, 10)]), 'kmh')[0].text).toBe('19');
  });
});
