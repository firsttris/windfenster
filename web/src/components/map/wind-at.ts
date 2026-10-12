/** The wind the map shows at a place, read on the CPU: the finest model whose texture holds it
 * (ICON-D2, the ICON global piece, ECMWF), blended between its two hours. */
import { blend, sampleWind, type Wind } from '@/lib/wind-map';
import { framePixels } from './frames';
import type { WindLayers } from './wind-gl';

export function windAt(layers: WindLayers, lon: number, lat: number): { wind: Wind; model: string } | null {
  for (const pair of [layers.detail, layers.mid, layers.base]) {
    if (!pair) continue;
    const a = sampleWind(pair.a.meta, framePixels(pair.a), lon, lat);
    if (!a) continue;
    const b = pair.t > 0 ? sampleWind(pair.b.meta, framePixels(pair.b), lon, lat) : a;
    const wind = blend(a, b, pair.t);
    if (wind) return { wind, model: pair.a.meta.model };
  }
  return null;
}
