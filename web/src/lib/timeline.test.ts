import { describe, expect, it } from 'vitest';
import { daySegments, hourAt, onDay } from './timeline';

// Local times, whatever zone the tests run in
const at = (y: number, mo: number, d: number, h: number) => new Date(y, mo - 1, d, h).getTime() / 1000;

describe('the timeline', () => {
  it('cuts the span into local days', () => {
    const segs = daySegments(at(2026, 10, 11, 15), at(2026, 10, 13, 6));
    expect(segs.map((s) => [s.from, s.to])).toEqual([
      [at(2026, 10, 11, 15), at(2026, 10, 12, 0)],
      [at(2026, 10, 12, 0), at(2026, 10, 13, 0)],
      [at(2026, 10, 13, 0), at(2026, 10, 13, 6)],
    ]);
    expect(segs[0].day).toBe(at(2026, 10, 11, 0));
  });

  it('jumps to the same hour of another day, within the span', () => {
    const from = at(2026, 10, 11, 15);
    const to = at(2026, 10, 13, 6);
    expect(onDay(at(2026, 10, 11, 18), at(2026, 10, 12, 0), from, to)).toBe(at(2026, 10, 12, 18));
    expect(onDay(at(2026, 10, 11, 18), at(2026, 10, 13, 0), from, to)).toBe(to);
    expect(onDay(at(2026, 10, 12, 9), at(2026, 10, 11, 0), from, to)).toBe(from);
  });

  it('finds the hour under the pointer', () => {
    const from = at(2026, 10, 11, 0);
    const to = at(2026, 10, 11, 10);
    expect(hourAt(0.26, from, to)).toBe(at(2026, 10, 11, 3));
    expect(hourAt(2, from, to)).toBe(to);
  });
});
