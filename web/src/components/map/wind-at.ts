/** The wind the map shows at a place, read on the CPU by the shaders' rules (`lib/wind-stack.ts`):
 * ICON-D2 fading into ICON global fading into ECMWF, each blended between its two hours. */
import { blend, sampleWind, type Wind } from '@/lib/wind-map';
import { alphaMip, EDGE_LOD, edgeOf, innerOf, layered } from '@/lib/wind-stack';
import { framePixels, type WindFrame } from './frames';
import type { WindLayers, WindPair } from './wind-gl';

// A frame's alpha at the edge's mip level, made once
const mips = new WeakMap<WindFrame, ReturnType<typeof alphaMip>>();
function mipOf(frame: WindFrame) {
  let mip = mips.get(frame);
  if (!mip) {
    mip = alphaMip(framePixels(frame), frame.meta.width, frame.meta.height, EDGE_LOD);
    mips.set(frame, mip);
  }
  return mip;
}

function pairAt(pair: WindPair | null, lon: number, lat: number): { wind: Wind; model: string } | null {
  if (!pair) return null;
  const a = sampleWind(pair.a.meta, framePixels(pair.a), lon, lat);
  const b = pair.t > 0 ? sampleWind(pair.b.meta, framePixels(pair.b), lon, lat) : a;
  const wind = blend(a, b, pair.t);
  return wind ? { wind, model: pair.a.meta.model } : null;
}

export function windAt(layers: WindLayers, lon: number, lat: number): { wind: Wind; model: string } | null {
  const { base, mid, detail } = layers;
  return layered(
    pairAt(base, lon, lat),
    pairAt(mid, lon, lat),
    pairAt(detail, lon, lat),
    mid ? innerOf(mid.a.meta, lon, lat) : 0,
    detail ? edgeOf(detail.a.meta, mipOf(detail.a), lon, lat) : 0,
  );
}
