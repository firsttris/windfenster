import { describe, expect, it } from 'vitest';
import { windBand } from '@/ui/wind-scale';
import { fits, glanceBand, peak } from './spot-glance';
import type { SpotHour } from './spot-series';

const h = (validAt: number, wind: number, dir: number): SpotHour => ({
  validAt,
  runAt: 0,
  wind,
  gust: null,
  dir,
  model: 'ecmwf_ifs025',
});

const west = { from: 225, to: 315 };
const hours = [h(0, 8, 270), h(3600, 22, 90), h(7200, 16, 250)];

describe('a spot at a glance', () => {
  it('lets every direction work without a sector', () => {
    expect(fits(hours[1], null)).toBe(true);
    expect(fits(hours[1], west)).toBe(false);
  });

  it('fades the hours from the wrong direction', () => {
    const band = glanceBand(hours, 0, 7200, west);
    expect(band.startsWith('linear-gradient(to right, ')).toBe(true);
    // The easterly 22 kn hour is faded, the westerlies are not.
    const strong = windBand(22).bg;
    expect(band).toContain(`color-mix(in srgb, ${strong} 28%, transparent)`);
    expect(glanceBand([], 0, 1, null)).toBe('transparent');
  });

  it('finds the strongest hour that works, from now on', () => {
    expect(peak(hours, west, 0)?.wind).toBe(16);
    expect(peak(hours, null, 0)?.wind).toBe(22);
    expect(peak(hours, west, 10_000)).toBeNull();
  });
});
