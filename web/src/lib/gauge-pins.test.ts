import { describe, expect, it } from 'vitest';
import { GAUGE_COLOR, gaugeProps } from './gauge-pins';

describe('gauge pins', () => {
  it('shows the level, the gauge and its water, coloured by the state', () => {
    const p = gaugeProps({
      uuid: 'a',
      name: 'Iffezheim',
      water: 'Rhein',
      lat: 48.85,
      lon: 8.12,
      levelCm: 311.6,
      state: 'high',
    });
    expect(p).toEqual({
      uuid: 'a',
      name: 'Iffezheim',
      color: GAUGE_COLOR.high,
      label: '312 cm',
      title: 'Iffezheim · Rhein',
    });
  });

  it('shows no level without a fresh reading, and the name alone without a water', () => {
    const p = gaugeProps({ uuid: 'b', name: 'Maxau', water: null, lat: 49, lon: 8.3, levelCm: null, state: 'unknown' });
    expect([p.label, p.title, p.color]).toEqual(['', 'Maxau', GAUGE_COLOR.unknown]);
  });

  it('tells the states apart', () => {
    expect(new Set(Object.values(GAUGE_COLOR)).size).toBe(4);
  });
});
