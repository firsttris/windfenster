import { describe, expect, it } from 'vitest';
import { FADE, takesStep, trailFade } from './trails';

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

describe('steps of the particles', () => {
  /** Steps taken over a second of frames every `ms` */
  const steps = (ms: number) => {
    let [last, n] = [0, 0];
    for (let i = 1; i <= Math.round(1000 / ms); i++) {
      const now = i * ms;
      if (takesStep(last, now)) [last, n] = [now, n + 1];
    }
    return n;
  };

  it('takes every frame at 60 Hz and every other at 120 and 144 Hz', () => {
    expect(steps(1000 / 60)).toBe(60);
    expect(steps(1000 / 120)).toBe(60);
    expect(steps(1000 / 144)).toBe(72);
    // A slow device: every frame it has
    expect(steps(1000 / 30)).toBe(30);
  });
});
