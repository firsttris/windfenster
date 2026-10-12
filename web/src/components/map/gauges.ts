/** The water gauges on the map (PEGELONLINE): from near zoom a small dot in the colour of the
 * level's state with the level beside it, from town zoom the gauge and its water under it. Under
 * the stations' pins, which they give way to. */
import type { GeoJSONSource, LayerSpecification, Map as MapLibreMap } from 'maplibre-gl';
import type { GaugePin } from '@/bindings/GaugePin';
import { GAUGE_ZOOM, gaugeProps } from '@/lib/gauge-pins';
import type { Theme } from '@/lib/prefs';
import type { PaintProperty } from './palette';

const SOURCE = 'gauges';

function layers(theme: Theme): LayerSpecification[] {
  const halo = theme === 'dark' ? 'rgba(9,9,11,0.85)' : 'rgba(255,255,255,0.92)';
  const muted = theme === 'dark' ? '#a1a1aa' : '#52525b';
  return [
    {
      id: 'gauges-dot',
      type: 'circle',
      source: SOURCE,
      minzoom: GAUGE_ZOOM,
      paint: {
        'circle-radius': ['interpolate', ['linear'], ['zoom'], GAUGE_ZOOM, 3.5, 13, 5],
        'circle-color': ['get', 'color'],
        'circle-stroke-color': theme === 'dark' ? '#09090b' : '#ffffff',
        'circle-stroke-width': 1.5,
        'circle-opacity': ['interpolate', ['linear'], ['zoom'], GAUGE_ZOOM, 0, GAUGE_ZOOM + 0.4, 1],
        'circle-stroke-opacity': ['interpolate', ['linear'], ['zoom'], GAUGE_ZOOM, 0, GAUGE_ZOOM + 0.4, 1],
      },
    },
    {
      // The level beside the dot; from town zoom the gauge and its water under it, in one label
      // so neither pushes the other out
      id: 'gauges-label',
      type: 'symbol',
      source: SOURCE,
      minzoom: GAUGE_ZOOM,
      layout: {
        'text-field': [
          'step',
          ['zoom'],
          ['get', 'label'],
          10,
          [
            'format',
            ['get', 'label'],
            {},
            '\n',
            {},
            ['get', 'title'],
            { 'font-scale': 0.82, 'text-font': ['literal', ['Noto Sans Regular']], 'text-color': muted },
          ],
        ],
        'text-font': ['Noto Sans Medium'],
        'text-size': ['interpolate', ['linear'], ['zoom'], GAUGE_ZOOM, 11, 13, 13],
        'text-anchor': 'left',
        'text-justify': 'left',
        'text-offset': [0.7, 0],
        'text-max-width': 14,
        'text-optional': true,
      },
      paint: {
        // The dot tells the state; the words in the ink stations' values use, readable on any field
        'text-color': theme === 'dark' ? '#fafafa' : '#18181b',
        'text-halo-color': halo,
        'text-halo-width': 1.4,
        'text-opacity': ['interpolate', ['linear'], ['zoom'], GAUGE_ZOOM, 0, GAUGE_ZOOM + 0.4, 1],
      },
    },
  ];
}

/** Adds or refreshes the gauges, under the stations' pins when those are there. */
export function setGauges(map: MapLibreMap, gauges: readonly GaugePin[], theme: Theme) {
  const data: Parameters<GeoJSONSource['setData']>[0] = {
    type: 'FeatureCollection',
    features: gauges.map((g) => ({
      type: 'Feature',
      properties: { ...gaugeProps(g) },
      geometry: { type: 'Point', coordinates: [g.lon, g.lat] },
    })),
  };
  const source = map.getSource<GeoJSONSource>(SOURCE);
  if (source) {
    void source.setData(data);
    return;
  }
  map.addSource(SOURCE, { type: 'geojson', data });
  const under = map.getLayer('pins-cluster') ? 'pins-cluster' : undefined;
  for (const l of layers(theme)) map.addLayer(l, under);
}

export function gaugeTheme(map: MapLibreMap, theme: Theme) {
  for (const l of layers(theme)) {
    if (!map.getLayer(l.id) || !('paint' in l) || !l.paint) continue;
    for (const [k, v] of Object.entries(l.paint)) map.setPaintProperty(l.id, k as PaintProperty, v);
  }
}
