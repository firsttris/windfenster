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
    expect(spotHours(series, ['icon_d2', 'ecmwf_ifs025']).map((p) => [p.validAt, p.model])).toEqual([
      [0, 'ecmwf_ifs025'],
      [3, 'icon_d2'],
      [4, 'icon_d2'],
      [6, 'ecmwf_ifs025'],
    ]);
    // Outside the finer model's domain it has no hours: the base alone
    const outside = { layers: [{ model: 'icon_d2', points: [] }, series.layers[1]] };
    expect(spotHours(outside, ['icon_d2', 'ecmwf_ifs025'])).toHaveLength(3);
    expect(spotHours(series, [null, 'ecmwf_ifs025'])).toHaveLength(3);
    // ICON global between the two: around ICON-D2, before ECMWF
    const three = { layers: [...series.layers, { model: 'icon_global', points: [h(0, 2), h(1, 3), h(5, 4)] }] };
    expect(spotHours(three, ['icon_d2', 'icon_global', 'ecmwf_ifs025']).map((p) => [p.validAt, p.model])).toEqual([
      [0, 'icon_global'],
      [1, 'icon_global'],
      [3, 'icon_d2'],
      [4, 'icon_d2'],
      [5, 'icon_global'],
      [6, 'ecmwf_ifs025'],
    ]);
  });

  it('colours the timeline by the wind', () => {
    const hours = spotHours(series, [null, 'ecmwf_ifs025']);
    const g = windGradient(hours, 0, 6);
    // 1 kn, 9 kn and 15 kn: the bands from 0, 8 and 14 kn
    expect(g).toBe(
      `linear-gradient(to right, ${WIND_SCALE[0].bg} 0.00%, ${WIND_SCALE[0].bg} 25.00%, ` +
        `${WIND_SCALE[4].bg} 25.00%, ${WIND_SCALE[4].bg} 75.00%, ${WIND_SCALE[7].bg} 75.00%, ${WIND_SCALE[7].bg} 100.00%)`,
    );
    expect(windGradient([], 0, 6)).toBe('transparent');
  });
});
