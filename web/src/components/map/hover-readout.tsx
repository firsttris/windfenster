/** On devices with a mouse: the model's wind and gusts under the pointer, live, without a
 * click. Its own component, so moving the mouse re-renders only this label. */
import type { Map as MapLibreMap, MapMouseEvent } from 'maplibre-gl';
import { useEffect, useState } from 'react';
import type { WindUnit } from '@/bindings/WindUnit';
import { formatWind, UNIT_LABEL } from '@/lib/units';
import { useMedia } from '@/lib/use-media';
import { gustOf, speedAndDirection } from '@/lib/wind-map';
import { m } from '@/paraglide/messages';
import { WindArrow } from '@/ui/wind-arrow';
import { windBand } from '@/ui/wind-scale';
import { windAt } from './wind-at';
import type { WindLayers } from './wind-gl';

interface Props {
  map: MapLibreMap;
  layers: WindLayers;
  unit: WindUnit;
  gusts: boolean;
}

interface Pointer {
  x: number;
  y: number;
  lon: number;
  lat: number;
}

/** Pixels between the pointer and the label */
const GAP = 16;

export function HoverReadout({ map, layers, unit, gusts }: Props) {
  const mouse = useMedia('(hover: hover) and (pointer: fine)', false);
  const [at, setAt] = useState<Pointer | null>(null);

  useEffect(() => {
    if (!mouse) return;
    let frame = 0;
    let last: MapMouseEvent | null = null;
    const move = (e: MapMouseEvent) => {
      last = e;
      if (frame) return;
      frame = requestAnimationFrame(() => {
        frame = 0;
        if (last) setAt({ x: last.point.x, y: last.point.y, lon: last.lngLat.lng, lat: last.lngLat.lat });
      });
    };
    const hide = () => {
      cancelAnimationFrame(frame);
      frame = 0;
      last = null;
      setAt(null);
    };
    map.on('mousemove', move);
    map.on('mouseout', hide);
    map.on('dragstart', hide);
    map.on('zoomstart', hide);
    return () => {
      map.off('mousemove', move);
      map.off('mouseout', hide);
      map.off('dragstart', hide);
      map.off('zoomstart', hide);
      cancelAnimationFrame(frame);
    };
  }, [map, mouse]);

  if (!at) return null;
  const here = windAt(layers, at.lon, at.lat);
  if (!here) return null;
  const { kn, from } = speedAndDirection(here.wind);
  const gust = here.wind.gust !== null ? gustOf(here.wind) : null;
  const band = windBand(gusts && gust !== null ? gust : kn);
  // Left of the pointer near the right edge
  const flip = at.x > map.getContainer().clientWidth - 200;
  return (
    <div
      aria-hidden="true"
      className="pointer-events-none absolute z-10 flex items-center gap-1.5 whitespace-nowrap rounded-lg border bg-card/95 px-2 py-1 text-xs shadow-md backdrop-blur"
      style={{
        left: flip ? undefined : at.x + GAP,
        right: flip ? map.getContainer().clientWidth - at.x + GAP : undefined,
        top: at.y + GAP,
      }}
    >
      <WindArrow deg={from} className="size-3.5" />
      <span className="rounded px-1 font-mono font-semibold" style={{ background: band.bg, color: band.fg }}>
        {formatWind(gusts && gust !== null ? gust : kn, unit)}
      </span>
      <span className="text-muted-foreground">
        {gusts && gust !== null
          ? `${UNIT_LABEL[unit]} · ${m.map_layerGusts()}`
          : gust !== null
            ? m.map_gust({ gust: formatWind(gust, unit), unit: UNIT_LABEL[unit] })
            : UNIT_LABEL[unit]}
      </span>
    </div>
  );
}
