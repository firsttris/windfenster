import { describe, expect, it } from 'vitest';
import type { MapSeries } from '@/bindings/MapSeries';
import { WIND_SCALE } from '@/ui/wind-scale';
import { spotHours, windGradient } from './spot-series';

const h = (validAt: number, wind: number) => ({ validAt, runAt: 0, wind, gust: wind + 5, dir: 270 });

const series: MapSeries = {
  layers: [
    { model: 'icon_d2', points: [h(3, 10), h(4, 12)] },
    { model: 'ecmwf_ifs025', points: [h(0, 1), h(3, 9), h(6, 15)] },
  ],
};

describe('a place over the hours', () => {
  it('takes the finer model while it lasts, the base around it', () => {
    expect(spotHours(series, 'ecmwf_ifs025', 'icon_d2').map((p) => [p.validAt, p.model])).toEqual([
      [0, 'ecmwf_ifs025'],
      [3, 'icon_d2'],
      [4, 'icon_d2'],
      [6, 'ecmwf_ifs025'],
    ]);
    // Outside the finer model's domain it has no hours: the base alone
    const outside = { layers: [{ model: 'icon_d2', points: [] }, series.layers[1]] };
    expect(spotHours(outside, 'ecmwf_ifs025', 'icon_d2')).toHaveLength(3);
    expect(spotHours(series, 'ecmwf_ifs025', null)).toHaveLength(3);
  });

  it('colours the timeline by the wind', () => {
    const hours = spotHours(series, 'ecmwf_ifs025', null);
    const g = windGradient(hours, 0, 6);
    // 1 kn, 9 kn and 15 kn: the first, third and fourth band
    expect(g).toBe(
      `linear-gradient(to right, ${WIND_SCALE[0].bg} 0.00%, ${WIND_SCALE[0].bg} 25.00%, ` +
        `${WIND_SCALE[2].bg} 25.00%, ${WIND_SCALE[2].bg} 75.00%, ${WIND_SCALE[3].bg} 75.00%, ${WIND_SCALE[3].bg} 100.00%)`,
    );
    expect(windGradient([], 0, 6)).toBe('transparent');
  });
});
