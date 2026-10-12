import { describe, expect, it } from 'vitest';
import { GLIDE_MS, glideAt, glides } from './glide';

describe('gliding between hours', () => {
  it('glides steps of up to three hours, either way', () => {
    expect(glides(0, 3600)).toBe(true);
    expect(glides(7200, 0)).toBe(true);
    expect(glides(0, 3 * 3600)).toBe(true);
    expect(glides(0, 3 * 3600 + 1)).toBe(false);
    expect(glides(3600, 3600)).toBe(false);
  });

  it('starts at the old hour, eases out and ends on the new one', () => {
    expect(glideAt(0, 3600, 0)).toBe(0);
    expect(glideAt(0, 3600, GLIDE_MS / 2)).toBeCloseTo(3600 * 0.875);
    expect(glideAt(0, 3600, GLIDE_MS)).toBe(3600);
    expect(glideAt(0, 3600, GLIDE_MS * 5)).toBe(3600);
    expect(glideAt(3600, 0, GLIDE_MS / 2)).toBeCloseTo(450);
    // Never past the new hour on the way
    for (let ms = 0; ms <= GLIDE_MS; ms += 10) expect(glideAt(0, 3600, ms)).toBeLessThanOrEqual(3600);
  });
});
