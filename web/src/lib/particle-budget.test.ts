import { describe, expect, it } from 'vitest';
import { countsAsFrame, nextBudget, particleCount, SLOW_MS } from './particle-budget';

describe('particle budget', () => {
  it('scales with the map and the budget, within limits', () => {
    expect(particleCount(1200, 800, 1)).toBe(2000);
    expect(particleCount(1200, 800, 0.5)).toBe(1500);
    expect(particleCount(3840, 2160, 1)).toBe(16_000);
    expect(particleCount(3840, 2160, 0.35)).toBe(6048);
  });

  it('shrinks when frames are slow, down to a floor', () => {
    expect(nextBudget(1, 16.7)).toBe(1);
    expect(nextBudget(1, SLOW_MS + 1)).toBeCloseTo(0.7, 9);
    expect(nextBudget(0.4, 40)).toBe(0.35);
  });

  it('ignores pauses and the first frame', () => {
    expect(countsAsFrame(16.7)).toBe(true);
    expect(countsAsFrame(8.3)).toBe(true);
    expect(countsAsFrame(0)).toBe(false);
    expect(countsAsFrame(2_000)).toBe(false);
  });
});
