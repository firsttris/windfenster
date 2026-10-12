import { describe, expect, it } from 'vitest';
import type { GearItem } from '@/bindings/GearItem';
import { gearFor, sizeText, template } from './gear';

const wing5: GearItem = { kind: 'wing', size: 5, minKn: 11, maxKn: 18 };
const wing4: GearItem = { kind: 'wing', size: 4, minKn: 14, maxKn: 30 };
const kite12: GearItem = { kind: 'kite', size: 12, minKn: 13, maxKn: 20 };

describe('gear for an hour', () => {
  it('takes the piece whose range the wind sits in most centrally', () => {
    // 17 kn: the 5 (11–18) is at its top, the 4 (14–30) a little way into its range
    expect(gearFor([wing5, wing4], 17, 18)).toEqual({ fits: wing4, gusty: false });
    expect(gearFor([wing5, wing4], 12, 14)).toEqual({ fits: wing5, gusty: false });
    // A kiter who wings too gets whatever fits best.
    expect(gearFor([wing5, kite12], 17, null)).toEqual({ fits: kite12, gusty: false });
  });

  it('takes a range of a single knot as one knot wide', () => {
    const exact: GearItem = { kind: 'kite', size: 9, minKn: 20, maxKn: 20 };
    expect(gearFor([exact], 20, null)).toEqual({ fits: exact, gusty: false });
    // Centred at 20 it fits better than the 4 at the top of its range
    expect(gearFor([wing4, exact], 20, null)).toEqual({ fits: exact, gusty: false });
  });

  it('says when the gusts go past the top of the range', () => {
    expect(gearFor([wing5], 16, 24)).toEqual({ fits: wing5, gusty: true });
  });

  it('says too little, too much or nothing in between', () => {
    expect(gearFor([wing5, wing4], 8, 10)).toBe('low');
    expect(gearFor([wing5, wing4], 35, 40)).toBe('high');
    expect(
      gearFor(
        [
          { ...wing5, maxKn: 13 },
          { ...wing4, minKn: 20 },
        ],
        16,
        null,
      ),
    ).toBe('none');
    expect(gearFor([], 16, null)).toBeNull();
  });

  it('writes sizes as riders say them', () => {
    expect([sizeText(4), sizeText(4.5), sizeText(7.5)]).toEqual(['4', '4.5', '7.5']);
  });

  it('fills a first list by kind and level', () => {
    const mid = template('wing', 'intermediate');
    const expert = template('wing', 'expert');
    expect(mid.every((g) => g.kind === 'wing')).toBe(true);
    expect(expert[0].minKn).toBe(mid[0].minKn - 3);
    expect(template('kite', 'beginner')[0].minKn).toBe(13);
  });
});
