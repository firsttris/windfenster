import type { LayerSpecification } from 'maplibre-gl';
import { describe, expect, it } from 'vitest';
import type { BasemapInfo } from '@/bindings/BasemapInfo';
import { boxes, splitLayers, splitPaint } from './basemap-detail';

const info: BasemapInfo = {
  minZoom: 0,
  maxZoom: 14,
  bounds: [-25, 27, 45, 72],
  fullZoom: 8,
  detail: [[7.8, 48.6, 8.4, 49]],
};

const layers: LayerSpecification[] = [
  { id: 'sea', type: 'background' },
  { id: 'earth', type: 'fill', source: 'pm', 'source-layer': 'earth' },
  { id: 'water', type: 'fill', source: 'pm', 'source-layer': 'water' },
  { id: 'water-inland', type: 'fill', source: 'pm', 'source-layer': 'water' },
  { id: 'coast', type: 'line', source: 'pm', 'source-layer': 'water' },
  { id: 'roads', type: 'line', source: 'pm', 'source-layer': 'roads', filter: ['==', ['get', 'kind'], 'highway'] },
  { id: 'places', type: 'symbol', source: 'pm', 'source-layer': 'places' },
];

describe('basemap with detail around the spots', () => {
  const out = splitLayers(layers, info);
  const byId = Object.fromEntries(out.map((l) => [l.id, l]));
  const inside = boxes(info.detail);

  it('stacks the near fills over the far ones, under everything drawn over the field', () => {
    expect(out.map((l) => l.id)).toEqual([
      'sea',
      'earth',
      'water',
      'earth-near',
      'water-near',
      'water-inland',
      'coast',
      'roads',
      'places',
      'roads-near',
      'places-near',
    ]);
    expect(byId['earth-near']).toMatchObject({ source: 'pm-near' });
  });

  it('splits lines and labels at the edges of the boxes', () => {
    expect(byId.roads).toMatchObject({ filter: ['all', ['==', ['get', 'kind'], 'highway'], ['!', inside]] });
    expect(byId['roads-near']).toMatchObject({
      source: 'pm-near',
      filter: ['all', ['==', ['get', 'kind'], 'highway'], inside],
    });
    expect(byId.places).toMatchObject({ filter: ['!', inside] });
    // Polygons cannot be split: from afar only
    expect(byId.coast).toEqual(layers[4]);
    expect(byId['coast-near']).toBeUndefined();
  });

  it('gives the near layers the same theme', () => {
    expect(splitPaint({ roads: { 'line-color': '#fff' } })).toEqual({
      roads: { 'line-color': '#fff' },
      'roads-near': { 'line-color': '#fff' },
    });
  });

  it('makes a closed ring per box', () => {
    expect(inside).toEqual([
      'within',
      {
        type: 'MultiPolygon',
        coordinates: [
          [
            [
              [7.8, 48.6],
              [8.4, 48.6],
              [8.4, 49],
              [7.8, 49],
              [7.8, 48.6],
            ],
          ],
        ],
      },
    ]);
  });
});
