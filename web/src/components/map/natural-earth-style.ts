/**
 * The fallback basemap: Europe from Natural Earth (public domain, scripts/natural-earth.mjs),
 * enough from afar, coarse from close up. Used while no detailed basemap is installed.
 */
import type { ExpressionSpecification, LayerSpecification, StyleSpecification } from 'maplibre-gl';
import naturalEarth from '@/assets/natural-earth.json?url';
import type { Palette, ThemePaint } from './palette';

/** The colour field goes under this layer: rivers, coastlines, borders and places lie above. */
export const NATURAL_EARTH_ABOVE_FIELD = 'rivers';

const kind = (k: string): ExpressionSpecification => ['==', ['get', 'k'], k];

/** Paint properties that change with the theme, per layer */
export function naturalEarthPaint(p: Palette): ThemePaint {
  return {
    sea: { 'background-color': p.sea },
    land: { 'fill-color': p.land },
    lakes: { 'fill-color': p.sea },
    rivers: { 'line-color': p.river },
    coast: { 'line-color': p.coast },
    borders: { 'line-color': p.border },
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

export function naturalEarthLayers(lang: string): LayerSpecification[] {
  return [
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
    place('places-minor', ['>', ['get', 'r'], 3], 5.5, 12, lang),
    place('places-major', ['<=', ['get', 'r'], 3], 3, 13, lang),
  ];
}

export function naturalEarthSources(): StyleSpecification['sources'] {
  return {
    ne: {
      type: 'geojson',
      data: new URL(naturalEarth, location.origin).href,
      attribution: '<a href="https://www.naturalearthdata.com/">Natural Earth</a>',
    },
  };
}
