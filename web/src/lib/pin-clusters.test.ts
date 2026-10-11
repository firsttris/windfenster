import { describe, expect, it } from 'vitest';
import { WIND_SCALE } from '@/ui/wind-scale';
import { bandColor, windText } from './pin-clusters';
import { MS_PER_KN } from './units';

describe('pin bundles', () => {
  it('colours by the wind scale, grey without a measurement', () => {
    const e = bandColor(['get', 'kn'], 'bg', '#52525b') as unknown[];
    expect(e.slice(0, 3)).toEqual(['case', ['<', ['get', 'kn'], 0], '#52525b']);
    const step = e[3] as unknown[];
    expect(step.slice(0, 3)).toEqual(['step', ['get', 'kn'], WIND_SCALE[0].bg]);
    // Every band after the first: its lower edge, then its colour
    expect(step.slice(3)).toEqual(WIND_SCALE.slice(1).flatMap((b) => [b.from, b.bg]));
  });

  it('writes the wind in the chosen unit', () => {
    expect(windText(['get', 'kn'], 'kn', 'de')).toEqual([
      'number-format',
      ['*', ['get', 'kn'], 1],
      { locale: 'de', 'min-fraction-digits': 0, 'max-fraction-digits': 0 },
    ]);
    const ms = windText(['get', 'kn'], 'ms', 'en') as unknown[];
    expect(ms[1]).toEqual(['*', ['get', 'kn'], MS_PER_KN]);
    expect(ms[2]).toMatchObject({ 'max-fraction-digits': 1 });
  });
});
