import { describe, expect, it } from 'vitest';
import { swipeAction } from './swipe';

describe('swiping the spot sheet', () => {
  it('opens, folds and closes', () => {
    expect(swipeAction(-60, 0, false)).toBe('expand');
    expect(swipeAction(60, 0, true)).toBe('collapse');
    expect(swipeAction(60, 0, false)).toBe('close');
    // A flick counts even when short
    expect(swipeAction(-10, -0.8, false)).toBe('expand');
    expect(swipeAction(10, 0.8, false)).toBe('close');
  });

  it('ignores a small move', () => {
    expect(swipeAction(15, 0.1, false)).toBe('stay');
    expect(swipeAction(-80, 0, true)).toBe('stay');
  });
});
