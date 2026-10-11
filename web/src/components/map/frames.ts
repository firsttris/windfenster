/** The model hours as the wind layers use them: description plus decoded texture, loaded once
 * and kept for the last few hours shown. */
import type { TextureMeta } from '@/bindings/TextureMeta';

export interface WindFrame {
  /** `{model}/{run}/{step}`, unique per texture */
  path: string;
  meta: TextureMeta;
  image: ImageBitmap;
}

const KEEP = 8;
const loaded = new Map<string, Promise<WindFrame>>();

async function fetchFrame(path: string): Promise<WindFrame> {
  const [meta, blob] = await Promise.all([
    fetch(`/api/map/${path}.json`).then((r) => {
      if (!r.ok) throw new Error(`${path}.json: HTTP ${r.status}`);
      return r.json() as Promise<TextureMeta>;
    }),
    fetch(`/api/map/${path}.png`).then((r) => {
      if (!r.ok) throw new Error(`${path}.png: HTTP ${r.status}`);
      return r.blob();
    }),
  ]);
  // The colours are wind, not a picture: decode them untouched.
  const image = await createImageBitmap(blob, { premultiplyAlpha: 'none', colorSpaceConversion: 'none' });
  return { path, meta, image };
}

/** A frame, from memory when it was loaded lately. */
export function loadFrame(path: string): Promise<WindFrame> {
  const hit = loaded.get(path);
  if (hit) {
    // Most recently used last
    loaded.delete(path);
    loaded.set(path, hit);
    return hit;
  }
  const p = fetchFrame(path);
  p.catch(() => loaded.delete(path));
  loaded.set(path, p);
  while (loaded.size > KEEP) {
    // The bitmap itself goes with the last layer that holds it.
    loaded.delete(loaded.keys().next().value as string);
  }
  return p;
}

const pixelCache = new Map<string, Uint8ClampedArray>();

/** The texture's RGBA bytes, for reading the wind at a spot. */
export function framePixels(frame: WindFrame): Uint8ClampedArray {
  const hit = pixelCache.get(frame.path);
  if (hit) return hit;
  const { width, height } = frame.meta;
  const canvas = new OffscreenCanvas(width, height);
  const ctx = canvas.getContext('2d', { willReadFrequently: true });
  if (!ctx) throw new Error('no 2d context');
  ctx.drawImage(frame.image, 0, 0);
  const data = ctx.getImageData(0, 0, width, height, { colorSpace: 'srgb' }).data;
  // The two hours being blended
  if (pixelCache.size >= 2) pixelCache.delete(pixelCache.keys().next().value as string);
  pixelCache.set(frame.path, data);
  return data;
}
