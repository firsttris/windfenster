import { describe, expect, it } from 'vitest';
import { FADE, trailFade } from './trails';

describe('trail fade', () => {
  it('fades by frame, a pan as standing still, a zoom faster', () => {
    expect(trailFade(1, [1, 1, 0, 0])).toBe(FADE);
    expect(trailFade(2, [1, 1, 0, 0])).toBeCloseTo(FADE * FADE);
    expect(trailFade(1, [1, 1, 0.2, -0.1])).toBe(FADE);
    // A tenth of a zoom level in a frame, in or out alike
    const z = 2 ** 0.1;
    expect(trailFade(1, [z, z, 0, 0])).toBeCloseTo(FADE * 0.6);
    expect(trailFade(1, [1 / z, 1 / z, 0, 0])).toBeCloseTo(FADE * 0.6);
    // Never faster than half
    expect(trailFade(1, [2, 2, 0, 0])).toBeCloseTo(FADE / 2);
  });
});
