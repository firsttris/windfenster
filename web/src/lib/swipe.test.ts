import { describe, expect, it } from 'vitest';
import { swipeAction } from './swipe';

describe('swiping the spot sheet', () => {
  it('raises it a step at a time, lowers it and closes it from the lowest step', () => {
    expect(swipeAction(-60, 0, 0, 2)).toBe('up');
    expect(swipeAction(-60, 0, 1, 2)).toBe('up');
    expect(swipeAction(60, 0, 2, 2)).toBe('down');
    expect(swipeAction(60, 0, 1, 2)).toBe('down');
    expect(swipeAction(60, 0, 0, 2)).toBe('close');
    // A flick counts even when short
    expect(swipeAction(-10, -0.8, 0, 2)).toBe('up');
    expect(swipeAction(10, 0.8, 0, 2)).toBe('close');
  });

  it('ignores a small move and stops at the top', () => {
    expect(swipeAction(15, 0.1, 0, 2)).toBe('stay');
    expect(swipeAction(-80, 0, 2, 2)).toBe('stay');
  });
});
