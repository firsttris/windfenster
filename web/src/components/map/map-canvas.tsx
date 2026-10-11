import 'maplibre-gl/dist/maplibre-gl.css';
import { LocateFixed, Minus, Plus } from 'lucide-react';
import { addProtocol, type GeoJSONSource, Map as MapLibreMap, setWorkerUrl } from 'maplibre-gl';
import workerUrl from 'maplibre-gl/dist/maplibre-gl-worker.mjs?url';
import { Protocol } from 'pmtiles';
import { useEffect, useRef, useState } from 'react';
import type { StationWind } from '@/bindings/StationWind';
import type { WindUnit } from '@/bindings/WindUnit';
import { getLocale } from '@/lib/i18n';
import { useTheme } from '@/lib/prefs';
import { useMedia } from '@/lib/use-media';
import { viewOf } from '@/lib/wind-map';
import { m } from '@/paraglide/messages';
import { Button } from '@/ui/button';
import { arrowField } from './arrows';
import { applyTheme, type Basemap, setDomain } from './basemap';
import { type Particles, particles } from './particles';
import { PIN_LAYERS, pinTheme, setPins } from './pins';
import { type WindField, windField } from './wind-field';
import type { WindLayers } from './wind-gl';

/** A spot someone tapped: a station's pin or any place. */
export interface Pick {
  lon: number;
  lat: number;
  slug: string | null;
}

interface Props {
  basemap: Basemap;
  layers: WindLayers;
  /** Colour by the gusts instead of the wind */
  gusts: boolean;
  domain: [number, number][] | null;
  pins: readonly StationWind[] | undefined;
  unit: WindUnit;
  picked: Pick | null;
  onPick: (p: Pick) => void;
  /** The middle of the view, after every move */
  onCenter: (lon: number, lat: number) => void;
}

// MapLibre looks for its worker next to its own module, which the bundle no longer has.
setWorkerUrl(workerUrl);
// pmtiles://… sources: tiles by byte range from one file
addProtocol('pmtiles', new Protocol().tile);

const FIELD_OPACITY = { dark: 0.85, light: 0.8 } as const;

/** Germany and its neighbours, ICON-D2's domain */
const START: [[number, number], [number, number]] = [
  [3, 45.5],
  [17.5, 55.5],
];

export function MapCanvas({ basemap, layers, gusts, domain, pins, unit, picked, onPick, onCenter }: Props) {
  const container = useRef<HTMLDivElement>(null);
  const map = useRef<MapLibreMap | null>(null);
  const field = useRef<WindField | null>(null);
  const parts = useRef<Particles | null>(null);
  const pick = useRef(onPick);
  pick.current = onPick;
  const center = useRef(onCenter);
  center.current = onCenter;
  const [ready, setReady] = useState(false);
  const [theme] = useTheme();
  const reduced = useMedia('(prefers-reduced-motion: reduce)', false);

  // biome-ignore lint/correctness/useExhaustiveDependencies: the map is made once; theme and layer follow below
  useEffect(() => {
    if (!container.current) return;
    const mp = new MapLibreMap({
      container: container.current,
      style: basemap.style(theme, getLocale()),
      bounds: START,
      // The view in the address (#zoom/lat/lon), to share or come back to
      hash: true,
      maxBounds: [
        [basemap.bounds[0], basemap.bounds[1]],
        [basemap.bounds[2], basemap.bounds[3]],
      ],
      minZoom: 2.5,
      maxZoom: basemap.maxZoom,
      dragRotate: false,
      pitchWithRotate: false,
      touchPitch: false,
      renderWorldCopies: false,
      attributionControl: false,
      locale: { 'Map.Title': m.map_title() },
    });
    mp.touchZoomRotate.disableRotation();
    mp.keyboard.disableRotation();
    map.current = mp;

    const f = windField('wind-field', FIELD_OPACITY[theme], gusts);
    field.current = f;

    // Particles and arrows get canvases of their own over the map, under its controls.
    const overlays: HTMLCanvasElement[] = [];
    const overlay = () => {
      const canvas = document.createElement('canvas');
      canvas.style.cssText = 'position:absolute;inset:0;width:100%;height:100%;pointer-events:none';
      mp.getContainer().insertBefore(canvas, mp.getContainer().querySelector('.maplibregl-control-container'));
      overlays.push(canvas);
      return canvas;
    };
    // Less motion asked for: arrows that stand still instead of particles
    parts.current = reduced ? arrowField(overlay()) : particles(overlay());
    parts.current?.setTheme(theme);
    parts.current?.setGusts(gusts);

    let zoom = mp.getZoom();
    const sync = () => {
      const b = mp.getBounds();
      const z = mp.getZoom();
      const el = mp.getCanvas();
      parts.current?.setView(
        viewOf(b.getWest(), b.getSouth(), b.getEast(), b.getNorth()),
        el.clientWidth,
        el.clientHeight,
        mp.getPixelRatio(),
        Math.abs(z - zoom) > 1e-6,
      );
      zoom = z;
    };
    mp.on('move', sync);
    mp.on('resize', sync);
    const middle = () => {
      const c = mp.getCenter();
      center.current(c.lng, c.lat);
    };
    mp.on('moveend', middle);
    mp.on('load', () => {
      mp.addLayer(f.layer, basemap.aboveField);
      mp.addSource('picked', { type: 'geojson', data: { type: 'FeatureCollection', features: [] } });
      mp.addLayer({
        id: 'picked',
        type: 'circle',
        source: 'picked',
        paint: {
          'circle-radius': 8,
          'circle-color': 'rgba(0,0,0,0)',
          'circle-stroke-width': 2,
          'circle-stroke-color': '#fafafa',
        },
      });
      sync();
      middle();
      setReady(true);
    });
    mp.on('click', (e) => {
      const hit = mp
        .queryRenderedFeatures(
          [
            [e.point.x - 8, e.point.y - 8],
            [e.point.x + 8, e.point.y + 8],
          ],
          { layers: PIN_LAYERS.filter((l) => mp.getLayer(l)) },
        )
        .at(0);
      if (hit && hit.geometry.type === 'Point') {
        const [lon, lat] = hit.geometry.coordinates;
        pick.current({ lon, lat, slug: String(hit.properties.slug) });
      } else {
        pick.current({ lon: e.lngLat.lng, lat: e.lngLat.lat, slug: null });
      }
    });
    for (const l of PIN_LAYERS) {
      mp.on('mouseenter', l, () => {
        mp.getCanvas().style.cursor = 'pointer';
      });
      mp.on('mouseleave', l, () => {
        mp.getCanvas().style.cursor = '';
      });
    }
    return () => {
      parts.current?.destroy();
      parts.current = null;
      for (const c of overlays) c.remove();
      field.current = null;
      mp.remove();
      map.current = null;
      setReady(false);
    };
  }, [reduced, basemap]);

  useEffect(() => {
    field.current?.show(layers);
    parts.current?.show(layers);
  }, [layers]);

  useEffect(() => {
    field.current?.setGusts(gusts);
    parts.current?.setGusts(gusts);
  }, [gusts]);

  useEffect(() => {
    if (ready && map.current) setDomain(map.current, domain);
  }, [ready, domain]);

  useEffect(() => {
    const mp = map.current;
    if (!ready || !mp) return;
    applyTheme(mp, basemap, theme);
    pinTheme(mp, theme);
    field.current?.setOpacity(FIELD_OPACITY[theme]);
    parts.current?.setTheme(theme);
    mp.setPaintProperty('picked', 'circle-stroke-color', theme === 'dark' ? '#fafafa' : '#09090b');
  }, [ready, theme, basemap]);

  useEffect(() => {
    if (ready && map.current && pins) setPins(map.current, pins, unit, theme);
  }, [ready, pins, unit, theme]);

  useEffect(() => {
    if (!ready || !map.current) return;
    void map.current.getSource<GeoJSONSource>('picked')?.setData({
      type: 'FeatureCollection',
      features: picked
        ? [{ type: 'Feature', properties: {}, geometry: { type: 'Point', coordinates: [picked.lon, picked.lat] } }]
        : [],
    });
  }, [ready, picked]);

  const locate = () => {
    navigator.geolocation?.getCurrentPosition((pos) => {
      const { longitude: lon, latitude: lat } = pos.coords;
      map.current?.flyTo({ center: [lon, lat], zoom: 8 });
      pick.current({ lon, lat, slug: null });
    });
  };

  return (
    <div className="relative h-full w-full">
      {/* MapLibre makes it position: relative, so it is sized, not inset */}
      <div ref={container} className="h-full w-full" />
      <div className="absolute top-3 right-3 flex flex-col gap-2">
        <Button variant="outline" size="icon" aria-label={m.map_zoomIn()} onClick={() => map.current?.zoomIn()}>
          <Plus aria-hidden="true" />
        </Button>
        <Button variant="outline" size="icon" aria-label={m.map_zoomOut()} onClick={() => map.current?.zoomOut()}>
          <Minus aria-hidden="true" />
        </Button>
        {'geolocation' in navigator && (
          <Button variant="outline" size="icon" aria-label={m.map_locate()} onClick={locate}>
            <LocateFixed aria-hidden="true" />
          </Button>
        )}
      </div>
    </div>
  );
}
