/**
 * The detailed basemap: OpenStreetMap vector tiles in the Protomaps basemap schema (v4), from
 * the operator's PMTiles file. Our own calm style rather than Protomaps' themes: land and water
 * under the wind field; rivers, roads, borders and places over it, translucent, so they read
 * on any wind colour. Layer names and kinds follow protomaps/basemaps (styles/src/base_layers.ts).
 */
import type { ExpressionSpecification, LayerSpecification, StyleSpecification } from 'maplibre-gl';
import type { Palette, ThemePaint } from './palette';

/** The colour field goes under this layer: rivers, roads, borders and places lie above. */
export const PROTOMAPS_ABOVE_FIELD = 'water-lines';

const SOURCE = 'pm';

/** The name in the viewer's language, else the local one */
function name(lang: string): ExpressionSpecification {
  return ['coalesce', ['get', `name:${lang}`], ['get', 'name']];
}

const width = (...stops: number[]): ExpressionSpecification => [
  'interpolate',
  ['exponential', 1.6],
  ['zoom'],
  ...stops,
];

export function protomapsPaint(p: Palette): ThemePaint {
  return {
    sea: { 'background-color': p.sea },
    earth: { 'fill-color': p.land },
    green: { 'fill-color': p.green },
    water: { 'fill-color': p.sea },
    'water-lines': { 'line-color': p.river },
    coast: { 'line-color': p.coast },
    'roads-minor': { 'line-color': p.minor },
    'roads-major': { 'line-color': p.major },
    'roads-highway': { 'line-color': p.highway },
    'borders-region': { 'line-color': p.region },
    borders: { 'line-color': p.border },
    'water-labels': { 'text-color': p.water, 'text-halo-color': p.halo },
    'places-locality': { 'text-color': p.text, 'text-halo-color': p.halo },
    'places-country': { 'text-color': p.text, 'text-halo-color': p.halo },
  };
}

export function protomapsLayers(lang: string): LayerSpecification[] {
  const road = (id: string, kinds: string[], minzoom: number, w: ExpressionSpecification): LayerSpecification => ({
    id,
    type: 'line',
    source: SOURCE,
    'source-layer': 'roads',
    minzoom,
    filter: ['all', ['in', ['get', 'kind'], ['literal', kinds]], ['!', ['has', 'is_tunnel']]],
    layout: { 'line-cap': 'round', 'line-join': 'round' },
    paint: { 'line-width': w },
  });
  return [
    { id: 'sea', type: 'background' },
    {
      id: 'earth',
      type: 'fill',
      source: SOURCE,
      'source-layer': 'earth',
      filter: ['==', ['geometry-type'], 'Polygon'],
    },
    {
      id: 'green',
      type: 'fill',
      source: SOURCE,
      'source-layer': 'landuse',
      minzoom: 8,
      filter: ['in', ['get', 'kind'], ['literal', ['park', 'forest', 'wood', 'nature_reserve', 'national_park']]],
      paint: { 'fill-opacity': ['interpolate', ['linear'], ['zoom'], 8, 0, 10, 1] },
    },
    {
      id: 'water',
      type: 'fill',
      source: SOURCE,
      'source-layer': 'water',
      filter: ['==', ['geometry-type'], 'Polygon'],
    },
    // — the wind field —
    {
      id: 'water-lines',
      type: 'line',
      source: SOURCE,
      'source-layer': 'water',
      minzoom: 8,
      filter: [
        'all',
        ['==', ['geometry-type'], 'LineString'],
        ['in', ['get', 'kind'], ['literal', ['river', 'canal']]],
      ],
      paint: { 'line-width': width(8, 0.6, 14, 2.5) },
    },
    {
      id: 'coast',
      type: 'line',
      source: SOURCE,
      'source-layer': 'water',
      filter: ['==', ['geometry-type'], 'Polygon'],
      paint: { 'line-width': ['interpolate', ['linear'], ['zoom'], 3, 0.6, 12, 1.4] },
    },
    road('roads-minor', ['minor_road'], 11, width(11, 0.4, 16, 3)),
    road('roads-major', ['major_road'], 7, width(7, 0.4, 12, 1.4, 16, 4)),
    road('roads-highway', ['highway'], 5, width(5, 0.5, 12, 1.8, 16, 5)),
    {
      id: 'borders-region',
      type: 'line',
      source: SOURCE,
      'source-layer': 'boundaries',
      minzoom: 5,
      filter: ['>', ['get', 'kind_detail'], 2],
      paint: { 'line-width': 0.8, 'line-dasharray': [2, 2] },
    },
    {
      id: 'borders',
      type: 'line',
      source: SOURCE,
      'source-layer': 'boundaries',
      filter: ['<=', ['get', 'kind_detail'], 2],
      paint: { 'line-width': ['interpolate', ['linear'], ['zoom'], 3, 0.6, 10, 1.3], 'line-dasharray': [3, 2] },
    },
    {
      id: 'water-labels',
      type: 'symbol',
      source: SOURCE,
      'source-layer': 'water',
      filter: ['in', ['get', 'kind'], ['literal', ['sea', 'ocean', 'bay', 'strait', 'lake']]],
      layout: {
        'text-field': name(lang),
        'text-font': ['Noto Sans Regular'],
        'text-size': ['interpolate', ['linear'], ['zoom'], 4, 11, 10, 13],
        'text-letter-spacing': 0.06,
        'text-max-width': 8,
      },
      paint: { 'text-halo-width': 1.2 },
    },
    {
      id: 'places-locality',
      type: 'symbol',
      source: SOURCE,
      'source-layer': 'places',
      filter: ['==', ['get', 'kind'], 'locality'],
      layout: {
        'text-field': name(lang),
        'text-font': [
          'case',
          ['<=', ['get', 'min_zoom'], 5],
          ['literal', ['Noto Sans Medium']],
          ['literal', ['Noto Sans Regular']],
        ],
        'symbol-sort-key': ['coalesce', ['get', 'sort_key'], ['get', 'min_zoom']],
        'text-size': [
          'interpolate',
          ['linear'],
          ['zoom'],
          4,
          ['case', ['>=', ['get', 'population_rank'], 13], 13, 10],
          8,
          ['case', ['>=', ['get', 'population_rank'], 11], 16, 11.5],
          12,
          ['case', ['>=', ['get', 'population_rank'], 9], 19, 13],
        ],
        'text-padding': ['interpolate', ['linear'], ['zoom'], 5, 4, 10, 10],
        'text-max-width': 8,
      },
      paint: { 'text-halo-width': 1.4, 'text-halo-blur': 0.4 },
    },
    {
      id: 'places-country',
      type: 'symbol',
      source: SOURCE,
      'source-layer': 'places',
      maxzoom: 7,
      filter: ['==', ['get', 'kind'], 'country'],
      layout: {
        'text-field': name(lang),
        'text-font': ['Noto Sans Medium'],
        'text-size': ['interpolate', ['linear'], ['zoom'], 2, 10, 6, 14],
        'text-transform': 'uppercase',
        'text-letter-spacing': 0.1,
        'text-max-width': 6,
      },
      paint: { 'text-halo-width': 1.4, 'text-opacity': 0.75 },
    },
  ];
}

export function protomapsSources(): StyleSpecification['sources'] {
  return {
    [SOURCE]: {
      type: 'vector',
      url: `pmtiles://${location.origin}/api/map/basemap.pmtiles`,
      attribution:
        '<a href="https://www.openstreetmap.org/copyright">© OpenStreetMap</a> · <a href="https://protomaps.com">Protomaps</a>',
    },
  };
}
