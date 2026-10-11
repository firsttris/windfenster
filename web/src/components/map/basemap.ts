/**
 * The map under the wind: Europe from Natural Earth (public domain, scripts/natural-earth.mjs)
 * with labels in Noto Sans. Land and water lie under the colour field; coastlines, borders,
 * rivers and places above it, so they stay readable over any wind.
 */
import type {
  ExpressionSpecification,
  GeoJSONSource,
  LayerSpecification,
  Map as MapLibreMap,
  StyleSpecification,
} from 'maplibre-gl';
import naturalEarth from '@/assets/natural-earth.json?url';
import type { Theme } from '@/lib/prefs';

/** Where the basemap has data: the map stays inside. */
export const BASEMAP_BOUNDS: [number, number, number, number] = [-25, 27, 45, 72];

/** The id of the layer the colour field goes under: everything from here on lies above it. */
export const ABOVE_FIELD = 'rivers';

interface Palette {
  sea: string;
  land: string;
  line: string;
  coast: string;
  border: string;
  river: string;
  text: string;
  halo: string;
}

const PALETTES: Record<Theme, Palette> = {
  dark: {
    sea: '#0b0d12',
    land: '#1b1c21',
    line: 'rgba(250,250,250,0.55)',
    coast: 'rgba(250,250,250,0.62)',
    border: 'rgba(250,250,250,0.42)',
    river: 'rgba(160,190,230,0.22)',
    text: '#f4f4f5',
    halo: 'rgba(9,9,11,0.78)',
  },
  light: {
    sea: '#d7e0ea',
    land: '#f4f4f5',
    line: 'rgba(24,24,27,0.6)',
    coast: 'rgba(24,24,27,0.55)',
    border: 'rgba(24,24,27,0.4)',
    river: 'rgba(40,90,150,0.25)',
    text: '#18181b',
    halo: 'rgba(255,255,255,0.85)',
  },
};

export type PaintProperty = Parameters<MapLibreMap['setPaintProperty']>[1];

const kind = (k: string): ExpressionSpecification => ['==', ['get', 'k'], k];

/** Paint properties that change with the theme, per layer */
function paint(p: Palette): Record<string, Record<string, string>> {
  return {
    sea: { 'background-color': p.sea },
    land: { 'fill-color': p.land },
    lakes: { 'fill-color': p.sea },
    rivers: { 'line-color': p.river },
    coast: { 'line-color': p.coast },
    borders: { 'line-color': p.border },
    domain: { 'line-color': p.line },
    'places-major': { 'text-color': p.text, 'text-halo-color': p.halo },
    'places-minor': { 'text-color': p.text, 'text-halo-color': p.halo },
  };
}

function place(
  id: string,
  filter: ExpressionSpecification,
  minzoom: number,
  size: number,
  lang: string,
): LayerSpecification {
  return {
    id,
    type: 'symbol',
    source: 'ne',
    minzoom,
    filter: ['all', kind('place'), filter],
    layout: {
      'text-field': ['get', lang === 'en' ? 'en' : 'de'],
      'text-font': [size > 12 ? 'Noto Sans Medium' : 'Noto Sans Regular'],
      'text-size': ['interpolate', ['linear'], ['zoom'], 4, size - 1, 9, size + 2],
      'symbol-sort-key': ['get', 'r'],
      'text-padding': 6,
    },
    paint: { 'text-halo-width': 1.4, 'text-halo-blur': 0.4 },
  };
}

export function basemapStyle(theme: Theme, lang: string): StyleSpecification {
  const p = PALETTES[theme];
  const layers: LayerSpecification[] = [
    { id: 'sea', type: 'background' },
    { id: 'land', type: 'fill', source: 'ne', filter: kind('land') },
    { id: 'lakes', type: 'fill', source: 'ne', filter: kind('lake') },
    {
      id: 'rivers',
      type: 'line',
      source: 'ne',
      minzoom: 5,
      filter: kind('river'),
      paint: { 'line-width': ['interpolate', ['linear'], ['zoom'], 5, 0.6, 10, 1.6] },
    },
    {
      id: 'coast',
      type: 'line',
      source: 'ne',
      filter: ['any', kind('land'), kind('lake')],
      paint: { 'line-width': ['interpolate', ['linear'], ['zoom'], 3, 0.7, 10, 1.6] },
    },
    {
      id: 'borders',
      type: 'line',
      source: 'ne',
      filter: kind('border'),
      paint: { 'line-width': ['interpolate', ['linear'], ['zoom'], 3, 0.6, 10, 1.2], 'line-dasharray': [3, 2] },
    },
    {
      id: 'domain',
      type: 'line',
      source: 'domain',
      paint: { 'line-width': 1.2, 'line-dasharray': [4, 3] },
    },
    place('places-minor', ['>', ['get', 'r'], 3], 5.5, 12, lang),
    place('places-major', ['<=', ['get', 'r'], 3], 3, 13, lang),
  ];
  const colours = paint(p);
  return {
    version: 8,
    glyphs: `${location.origin}/glyphs/{fontstack}/{range}.pbf`,
    sources: {
      ne: {
        type: 'geojson',
        data: new URL(naturalEarth, location.origin).href,
        attribution: '<a href="https://www.naturalearthdata.com/">Natural Earth</a>',
      },
      domain: { type: 'geojson', data: { type: 'FeatureCollection', features: [] } },
    },
    layers: layers.map(
      (l) => ({ ...l, paint: { ...('paint' in l ? l.paint : {}), ...colours[l.id] } }) as LayerSpecification,
    ),
  };
}

/** Recolours the basemap for the other theme, leaving the wind layers in place. */
export function applyTheme(map: MapLibreMap, theme: Theme) {
  for (const [layer, props] of Object.entries(paint(PALETTES[theme]))) {
    if (!map.getLayer(layer)) continue;
    for (const [prop, value] of Object.entries(props)) map.setPaintProperty(layer, prop as PaintProperty, value);
  }
}

/** The model's domain as the dashed line. */
export function setDomain(map: MapLibreMap, ring: [number, number][] | null) {
  void map.getSource<GeoJSONSource>('domain')?.setData({
    type: 'FeatureCollection',
    features: ring?.length
      ? [{ type: 'Feature', properties: {}, geometry: { type: 'LineString', coordinates: [...ring, ring[0]] } }]
      : [],
  });
}
