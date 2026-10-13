import { describe, expect, it } from 'vitest';
import { nowIndex, overHours } from './day-now';

const c = (time: number, past: boolean) => ({ time, past });

describe('now in a day', () => {
  it('falls on the first hour to come', () => {
    expect(nowIndex([c(1, true), c(2, true), c(3, false), c(4, false)])).toBe(2);
  });

  it('is nowhere on a day all over or all to come', () => {
    expect(nowIndex([c(1, true), c(2, true)])).toBe(-1);
    expect(nowIndex([c(1, false), c(2, false)])).toBe(-1);
  });

  it('counts hours without a value before an hour that is over as over', () => {
    const day = [c(0, false), c(0, false), c(5, true), c(6, true)];
    expect(overHours(day)).toEqual([true, true, true, true]);
    expect(nowIndex(day)).toBe(-1);
    expect(nowIndex([c(0, false), c(5, true), c(6, false)])).toBe(2);
  });
});
