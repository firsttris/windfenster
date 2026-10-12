/** The stations on the map: from afar bundled, each bundle showing the strongest wind in it;
 * a dot in the colour of the measured wind, from closer up an arrow the way it blows and the
 * value; stations without a recent measurement as grey dots. From near zoom each station is a
 * badge as Windfinder's pins: a disc in the wind's colour with a light ring and a shadow, the
 * arrow in the colour that reads on it, so a calm station stands out on a dark map too. */
import type {
  ExpressionSpecification,
  GeoJSONSource,
  LayerSpecification,
  MapGeoJSONFeature,
  Map as MapLibreMap,
} from 'maplibre-gl';
import type { StationWind } from '@/bindings/StationWind';
import type { WindUnit } from '@/bindings/WindUnit';
import { intlLocale } from '@/lib/i18n';
import { bandColor, CLUSTER_MAX_ZOOM, CLUSTER_RADIUS, NO_WIND, windText } from '@/lib/pin-clusters';
import type { Theme } from '@/lib/prefs';
import { formatWind } from '@/lib/units';
import { windBand } from '@/ui/wind-scale';
import type { PaintProperty } from './palette';

export const PIN_LAYERS = ['pins-cluster', 'pins', 'pins-far', 'pins-arrow', 'pins-badge'];

/** Single stations, not bundles */
const SINGLE: ExpressionSpecification = ['!', ['has', 'point_count']];
const NO_LIVE = '#52525b';

const ARROW = 'wind-arrow';
/** From this zoom on, stations with a direction show an arrow instead of a dot. */
const ARROW_ZOOM = 6.5;
/** From this zoom on, every station is a badge. */
const BADGE_ZOOM = 8.5;
/** The badge's radius in pixels by zoom, plus `more` */
const badgeRadius = (more = 0): ExpressionSpecification => [
  'interpolate',
  ['linear'],
  ['zoom'],
  BADGE_ZOOM,
  10 + more,
  12,
  12.5 + more,
  15,
  14 + more,
];

/** An arrow pointing up, as a soft-edged mask: MapLibre tints it (SDF) and draws its outline. */
function arrowImage(): ImageData {
  const size = 44;
  const c = new OffscreenCanvas(size, size);
  const ctx = c.getContext('2d') as OffscreenCanvasRenderingContext2D;
  ctx.filter = 'blur(1.6px)';
  ctx.beginPath();
  ctx.moveTo(22, 5);
  ctx.lineTo(34, 37);
  ctx.lineTo(22, 29);
  ctx.lineTo(10, 37);
  ctx.closePath();
  ctx.fillStyle = '#ffffff';
  ctx.fill();
  return ctx.getImageData(0, 0, size, size);
}

function features(pins: readonly StationWind[], unit: WindUnit): Parameters<GeoJSONSource['setData']>[0] {
  return {
    type: 'FeatureCollection',
    features: pins.map((p) => ({
      type: 'Feature',
      properties: {
        slug: p.slug,
        name: p.name,
        live: p.wind !== null ? 1 : 0,
        kn: p.wind ?? NO_WIND,
        color: p.wind !== null ? windBand(p.wind).bg : '#71717a',
        ink: p.wind !== null ? windBand(p.wind).fg : '#f4f4f5',
        // Arrows point where the wind goes.
        rotate: p.dir !== null ? (p.dir + 180) % 360 : 0,
        arrow: p.dir !== null && p.wind !== null && p.wind >= 1 ? 1 : 0,
        label: p.wind !== null ? formatWind(p.wind, unit) : '',
      },
      geometry: { type: 'Point', coordinates: [p.lon, p.lat] },
    })),
  };
}

function clusterLabel(unit: WindUnit): ExpressionSpecification {
  return ['case', ['<', ['get', 'kn'], 0], '', windText(['get', 'kn'], unit, intlLocale())];
}

function layers(theme: Theme, unit: WindUnit): LayerSpecification[] {
  const edge = theme === 'dark' ? '#09090b' : '#ffffff';
  const dot = (id: string, filter: ExpressionSpecification, maxzoom: number): LayerSpecification => ({
    id,
    type: 'circle',
    source: 'pins',
    maxzoom,
    filter: ['all', SINGLE, filter],
    paint: {
      'circle-radius': ['interpolate', ['linear'], ['zoom'], 3, 1.6, 6, 3, 8, 5, 10, 7],
      'circle-color': ['get', 'color'],
      'circle-stroke-color': edge,
      'circle-stroke-width': ['interpolate', ['linear'], ['zoom'], 3, 0.4, 8, 1.4],
    },
  });
  return [
    {
      id: 'pins-cluster',
      type: 'circle',
      source: 'pins',
      maxzoom: CLUSTER_MAX_ZOOM + 1,
      filter: ['has', 'point_count'],
      paint: {
        // Larger the more stations it holds
        'circle-radius': ['step', ['get', 'point_count'], 11, 10, 13, 50, 15.5, 100, 18],
        'circle-color': bandColor(['get', 'kn'], 'bg', NO_LIVE),
        'circle-stroke-color': edge,
        'circle-stroke-width': 1.5,
      },
    },
    {
      id: 'pins-cluster-label',
      type: 'symbol',
      source: 'pins',
      maxzoom: CLUSTER_MAX_ZOOM + 1,
      filter: ['has', 'point_count'],
      layout: {
        'text-field': clusterLabel(unit),
        'text-font': ['Noto Sans Medium'],
        'text-size': 12,
        'text-allow-overlap': true,
        'text-ignore-placement': true,
      },
      paint: { 'text-color': bandColor(['get', 'kn'], 'fg', '#e4e4e7') },
    },
    // Calm or variable: a dot up to the badges
    dot('pins', ['all', ['==', ['get', 'live'], 1], ['==', ['get', 'arrow'], 0]], BADGE_ZOOM),
    // With a direction: a dot from afar, an arrow from closer up
    dot('pins-far', ['==', ['get', 'arrow'], 1], ARROW_ZOOM),
    {
      id: 'pins-arrow',
      type: 'symbol',
      source: 'pins',
      minzoom: ARROW_ZOOM,
      maxzoom: BADGE_ZOOM,
      filter: ['all', SINGLE, ['==', ['get', 'arrow'], 1]],
      layout: {
        'icon-image': ARROW,
        'icon-size': ['interpolate', ['linear'], ['zoom'], ARROW_ZOOM, 1.1, 10, 1.5, 13, 1.9],
        'icon-rotate': ['get', 'rotate'],
        'icon-rotation-alignment': 'map',
        'icon-allow-overlap': true,
        'icon-ignore-placement': true,
      },
      paint: { 'icon-color': ['get', 'color'], 'icon-halo-color': edge, 'icon-halo-width': 2 },
    },
    {
      id: 'pins-shadow',
      type: 'circle',
      source: 'pins',
      minzoom: BADGE_ZOOM,
      filter: SINGLE,
      paint: {
        'circle-radius': badgeRadius(3),
        'circle-color': 'rgba(0,0,0,0.45)',
        'circle-blur': 0.5,
        'circle-translate': [0, 1.5],
      },
    },
    {
      id: 'pins-badge',
      type: 'circle',
      source: 'pins',
      minzoom: BADGE_ZOOM,
      filter: SINGLE,
      paint: {
        'circle-radius': badgeRadius(),
        'circle-color': ['get', 'color'],
        'circle-stroke-color': '#ffffff',
        'circle-stroke-width': 2.5,
      },
    },
    {
      id: 'pins-badge-arrow',
      type: 'symbol',
      source: 'pins',
      minzoom: BADGE_ZOOM,
      filter: ['all', SINGLE, ['==', ['get', 'arrow'], 1]],
      layout: {
        'icon-image': ARROW,
        'icon-size': ['interpolate', ['linear'], ['zoom'], BADGE_ZOOM, 0.85, 12, 1.05, 15, 1.2],
        'icon-rotate': ['get', 'rotate'],
        'icon-rotation-alignment': 'map',
        'icon-allow-overlap': true,
        'icon-ignore-placement': true,
      },
      paint: { 'icon-color': ['get', 'ink'] },
    },
    {
      // From town zoom on the station's name under its value
      id: 'pins-name',
      type: 'symbol',
      source: 'pins',
      minzoom: 9.5,
      filter: SINGLE,
      layout: {
        'text-field': ['get', 'name'],
        'text-font': ['Noto Sans Regular'],
        'text-size': 11,
        'text-offset': ['interpolate', ['linear'], ['zoom'], 9.5, ['literal', [1.4, 1.5]], 15, ['literal', [1.5, 1.6]]],
        'text-padding': 0,
        'text-anchor': 'left',
        'text-max-width': 10,
        'text-optional': true,
      },
      paint: {
        'text-color': theme === 'dark' ? '#a1a1aa' : '#52525b',
        'text-halo-color': theme === 'dark' ? 'rgba(9,9,11,0.8)' : 'rgba(255,255,255,0.9)',
        'text-halo-width': 1.4,
      },
    },
    {
      id: 'pins-label',
      type: 'symbol',
      source: 'pins',
      minzoom: 7,
      filter: ['all', SINGLE, ['==', ['get', 'live'], 1]],
      layout: {
        'text-field': ['get', 'label'],
        'text-font': ['Noto Sans Medium'],
        'text-size': ['interpolate', ['linear'], ['zoom'], 7, 12, 12, 14],
        'text-offset': [
          'interpolate',
          ['linear'],
          ['zoom'],
          BADGE_ZOOM - 0.01,
          ['literal', [1.3, 0]],
          BADGE_ZOOM,
          ['literal', [1.4, -0.1]],
          15,
          ['literal', [1.5, -0.1]],
        ],
        'text-anchor': 'left',
        'text-allow-overlap': false,
      },
      paint: {
        'text-color': theme === 'dark' ? '#fafafa' : '#18181b',
        'text-halo-color': theme === 'dark' ? 'rgba(9,9,11,0.8)' : 'rgba(255,255,255,0.9)',
        'text-halo-width': 1.4,
      },
    },
  ];
}

/** Adds or refreshes the pins. */
export function setPins(map: MapLibreMap, pins: readonly StationWind[], unit: WindUnit, theme: Theme) {
  const data = features(pins, unit);
  const source = map.getSource<GeoJSONSource>('pins');
  if (source) {
    void source.setData(data);
    map.setLayoutProperty('pins-cluster-label', 'text-field', clusterLabel(unit));
    return;
  }
  if (!map.hasImage(ARROW)) map.addImage(ARROW, arrowImage(), { pixelRatio: 2, sdf: true });
  map.addSource('pins', {
    type: 'geojson',
    data,
    cluster: true,
    clusterMaxZoom: CLUSTER_MAX_ZOOM,
    clusterRadius: CLUSTER_RADIUS,
    // The strongest measured wind of the bundle
    clusterProperties: { kn: ['max', ['get', 'kn']] },
  });
  for (const l of layers(theme, unit)) map.addLayer(l);
}

/** Zooms in until a tapped bundle falls apart. */
export async function openCluster(map: MapLibreMap, feature: MapGeoJSONFeature) {
  const source = map.getSource<GeoJSONSource>('pins');
  const id = feature.properties.cluster_id;
  if (!source || typeof id !== 'number' || feature.geometry.type !== 'Point') return;
  const zoom = await source.getClusterExpansionZoom(id);
  map.easeTo({ center: feature.geometry.coordinates as [number, number], zoom });
}

export function pinTheme(map: MapLibreMap, theme: Theme) {
  for (const l of layers(theme, 'kn')) {
    if (!map.getLayer(l.id) || !('paint' in l) || !l.paint) continue;
    for (const [k, v] of Object.entries(l.paint)) map.setPaintProperty(l.id, k as PaintProperty, v);
  }
}
