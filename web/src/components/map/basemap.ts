/**
 * The map under the wind: the detailed basemap (Protomaps, served by tile) when there is one,
 * else the world from Natural Earth. Both with labels in Noto Sans, recoloured for the theme
 * without touching the wind layers on top.
 */
import type { GeoJSONSource, LayerSpecification, Map as MapLibreMap, StyleSpecification } from 'maplibre-gl';
import type { BasemapInfo } from '@/bindings/BasemapInfo';
import type { Theme } from '@/lib/prefs';
import {
  NATURAL_EARTH_ABOVE_FIELD,
  naturalEarthLayers,
  naturalEarthPaint,
  naturalEarthSources,
} from './natural-earth-style';
import { PALETTES, type PaintProperty, type ThemePaint } from './palette';
import {
  PROTOMAPS_ABOVE_FIELD,
  PROTOMAPS_SOURCE,
  protomapsLayers,
  protomapsPaint,
  protomapsSources,
} from './protomaps-style';

/** The whole world, as far as Web Mercator goes */
export const WORLD_BOUNDS: [number, number, number, number] = [-180, -85.051129, 180, 85.051129];
/** Deepest zoom the server sends basemap tiles for: the spots' (cut from above elsewhere) */
const TILE_ZOOM = 14;

/** One of the two basemaps */
export interface Basemap {
  style(theme: Theme, lang: string): StyleSpecification;
  /** The layer the colour field goes under */
  aboveField: string;
  paint(theme: Theme): ThemePaint;
  bounds: [number, number, number, number];
  /** Closest zoom worth showing: vector tiles stay sharp a few levels past their data */
  maxZoom: number;
  /** The vector source to load again when the server has new detail */
  tileSource?: string;
}

/** Whether the basemap goes once around the world, so the map shows its copies side by side */
export function isWorld(b: Pick<Basemap, 'bounds'>): boolean {
  return b.bounds[2] - b.bounds[0] >= 359;
}

const DOMAIN: LayerSpecification = {
  id: 'domain',
  type: 'line',
  source: 'domain',
  paint: { 'line-width': 1.2, 'line-dasharray': [4, 3] },
};

function withDomain(p: ThemePaint, line: string): ThemePaint {
  return { ...p, domain: { 'line-color': line } };
}

function style(
  sources: StyleSpecification['sources'],
  layers: LayerSpecification[],
  paint: ThemePaint,
): StyleSpecification {
  return {
    version: 8,
    glyphs: `${location.origin}/glyphs/{fontstack}/{range}.pbf`,
    sources: { ...sources, domain: { type: 'geojson', data: { type: 'FeatureCollection', features: [] } } },
    // The domain's dashes over the field and the borders, under the labels
    layers: [...layers.filter((l) => l.type !== 'symbol'), DOMAIN, ...layers.filter((l) => l.type === 'symbol')].map(
      (l) => ({ ...l, paint: { ...('paint' in l ? l.paint : {}), ...paint[l.id] } }) as LayerSpecification,
    ),
  };
}

/** The basemap to draw: the detailed one if the server has it. */
export function basemapFor(info: BasemapInfo | null): Basemap {
  if (info) {
    // Tiles of any zoom from the server: the world, the region deeper, the spots deeper still
    const zoom = Math.max(info.maxZoom, info.detailZoom, TILE_ZOOM);
    const bounds = info.bounds as [number, number, number, number];
    const paint = (theme: Theme) => withDomain(protomapsPaint(PALETTES[theme]), PALETTES[theme].line);
    return {
      style: (theme, lang) => style(protomapsSources(zoom, bounds), protomapsLayers(lang), paint(theme)),
      aboveField: PROTOMAPS_ABOVE_FIELD,
      paint,
      bounds,
      maxZoom: Math.min(16, zoom + 3),
      tileSource: PROTOMAPS_SOURCE,
    };
  }
  return {
    style: (theme, lang) =>
      style(
        naturalEarthSources(),
        naturalEarthLayers(lang),
        withDomain(naturalEarthPaint(PALETTES[theme]), PALETTES[theme].line),
      ),
    aboveField: NATURAL_EARTH_ABOVE_FIELD,
    paint: (theme) => withDomain(naturalEarthPaint(PALETTES[theme]), PALETTES[theme].line),
    bounds: WORLD_BOUNDS,
    maxZoom: 11,
  };
}

/** Recolours the basemap for the other theme, leaving the wind layers in place. */
export function applyTheme(map: MapLibreMap, basemap: Basemap, theme: Theme) {
  for (const [layer, props] of Object.entries(basemap.paint(theme))) {
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
