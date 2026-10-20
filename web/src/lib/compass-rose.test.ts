import { describe, expect, it } from 'vitest';
import { arc, at, variation } from './compass-rose';

describe('the wind compass', () => {
  it('puts north on top and goes clockwise', () => {
    expect(at(0, 10, 50)).toBe('50 40');
    expect(at(90, 10, 50)).toBe('60 50');
    expect(at(180, 10, 50)).toBe('50 60');
    expect(at(270, 10, 50)).toBe('40 50');
  });

  it('draws arcs clockwise, the long way where the range is wide, across north too', () => {
    expect(arc(0, 90, 10, 50)).toBe('M50 40 A10 10 0 0 1 60 50');
    expect(arc(90, 0, 10, 50)).toBe('M60 50 A10 10 0 1 1 50 40');
    expect(arc(300, 60, 10, 50)).toMatch(/ 0 0 1 /);
    expect(arc(45, 45, 10, 50)).toMatch(/A10 10 0 1 1 .* A10 10 0 1 1 /);
  });

  it('reads the variation off the METAR', () => {
    expect(variation('METAR EDSB 201950Z 24008KT 200V280 9999 FEW040 12/07 Q1015')).toEqual({ from: 200, to: 280 });
    expect(variation('METAR EDSB 201950Z VRB02KT CAVOK 11/09 Q1010')).toBeNull();
    expect(variation(null)).toBeNull();
  });
});
