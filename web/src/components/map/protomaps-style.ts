/**
 * The detailed basemap: OpenStreetMap vector tiles in the Protomaps basemap schema (v4), from
 * the operator's PMTiles file. Our own calm style rather than Protomaps' themes: land and water
 * under the wind field; rivers, roads, borders and places over it, translucent, so they read
 * on any wind colour. From town zoom on, where the field fades out, it turns into a map: land
 * use, clear water, buildings. Layer names and kinds follow protomaps/basemaps
 * (styles/src/base_layers.ts, tiles/…/layers/Landuse.java).
 */
import type { ExpressionSpecification, LayerSpecification, StyleSpecification } from 'maplibre-gl';
import { TOWN_FROM, TOWN_TO } from '@/lib/town-zoom';
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

/** `far` under the wind field, `near` from town zoom on */
const near = (far: string, close: string): ExpressionSpecification => [
  'interpolate',
  ['linear'],
  ['zoom'],
  TOWN_FROM,
  far,
  TOWN_TO,
  close,
];

/** Land use kinds by the colour they get */
const LANDUSE = {
  forest: ['forest', 'wood'],
  meadow: [
    'park',
    'national_park',
    'nature_reserve',
    'protected_area',
    'grassland',
    'grass',
    'scrub',
    'golf_course',
    'cemetery',
    'allotments',
    'village_green',
    'playground',
  ],
  farmland: ['farmland'],
  industrial: ['industrial'],
  aerodrome: ['aerodrome', 'airfield', 'military', 'naval_base', 'runway', 'taxiway'],
  sand: ['sand', 'beach'],
} as const;

function landuseColor(p: Palette): ExpressionSpecification {
  const kinds = Object.keys(LANDUSE) as (keyof typeof LANDUSE)[];
  const match = (colour: (k: keyof typeof LANDUSE) => string) =>
    [
      'match',
      ['get', 'kind'],
      ...kinds.flatMap((k) => [[...LANDUSE[k]], colour(k)]),
      p.land,
    ] as unknown as ExpressionSpecification;
  // Far out only the green of forests and parks shows, faintly, as before. MapLibre allows the
  // zoom only at the top of an expression: one match per end.
  return [
    'interpolate',
    ['linear'],
    ['zoom'],
    TOWN_FROM,
    match((k) => (k === 'forest' || k === 'meadow' ? p.green : p.land)),
    TOWN_TO,
    match((k) => p.near[k]),
  ];
}

export function protomapsPaint(p: Palette): ThemePaint {
  return {
    sea: { 'background-color': near(p.sea, p.near.sea) },
    earth: { 'fill-color': near(p.land, p.near.land) },
    landuse: { 'fill-color': landuseColor(p) },
    water: { 'fill-color': near(p.sea, p.near.sea) },
    buildings: { 'fill-color': p.near.buildings },
    'water-lines': { 'line-color': near(p.river, p.near.river) },
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
      id: 'landuse',
      type: 'fill',
      source: SOURCE,
      'source-layer': 'landuse',
      minzoom: 8,
      filter: ['in', ['get', 'kind'], ['literal', Object.values(LANDUSE).flat()]],
      paint: { 'fill-opacity': ['interpolate', ['linear'], ['zoom'], 8, 0, 10, 1] },
    },
    {
      id: 'water',
      type: 'fill',
      source: SOURCE,
      'source-layer': 'water',
      filter: ['==', ['geometry-type'], 'Polygon'],
    },
    {
      id: 'buildings',
      type: 'fill',
      source: SOURCE,
      'source-layer': 'buildings',
      minzoom: 13,
      paint: { 'fill-opacity': ['interpolate', ['linear'], ['zoom'], 13, 0, 14.5, 1] },
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
      paint: { 'line-width': width(8, 0.6, 14, 3) },
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
