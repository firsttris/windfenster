/** What both wind layers share on the GPU: the hours on show as textures and the uniforms the
 * `wind()` GLSL function reads. */
import { wrapsAround } from '@/lib/wind-map';
import type { WindFrame } from './frames';
import { bindTexture, createTexture, type Program } from './gl';

/** Two hours of a model and how far between them (0…1). */
export interface WindPair {
  a: WindFrame;
  b: WindFrame;
  t: number;
}

/** What the map shows: the base model over the whole world (ECMWF), a finer global one over it
 * where the app has its piece (ICON global), and the finest inside its domain (ICON-D2). Any may
 * be missing. */
export interface WindLayers {
  base: WindPair | null;
  mid: WindPair | null;
  detail: WindPair | null;
}

export const NO_WIND: WindLayers = { base: null, mid: null, detail: null };

const NAMES = ['base', 'mid', 'detail'] as const;

/** Texture units the wind takes, from 0: two hours of each layer */
export const WIND_UNITS = NAMES.length * 2;

export function hasWind(l: WindLayers): boolean {
  return NAMES.some((n) => l[n] !== null);
}

/** The frames of the layers, finest first */
export function framesOf(l: WindLayers): WindFrame[] {
  return [l.detail, l.mid, l.base].flatMap((p) => (p ? [p.a, p.b] : []));
}

/** Textures per frame in one GL context, uploaded when first drawn, with mipmaps: the field is
 * often shown smaller than its texels, and the detail's edge is read from a coarse level. A
 * global texture repeats east and west. */
export function windTextures(gl: WebGL2RenderingContext) {
  const textures = new Map<string, WebGLTexture>();
  // Stands in for a model that is not shown: no wind, outside every domain
  const empty = createTexture(gl, gl.LINEAR, new Uint8Array(4), 1, 1);
  return {
    get(frame: WindFrame | undefined): WebGLTexture {
      if (!frame) return empty;
      let t = textures.get(frame.path);
      if (!t) {
        t = createTexture(gl, gl.LINEAR, frame.image);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR_MIPMAP_LINEAR);
        // Around the world the east edge continues at the west one.
        if (wrapsAround(frame.meta)) gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.REPEAT);
        gl.generateMipmap(gl.TEXTURE_2D);
        textures.set(frame.path, t);
      }
      return t;
    },
    /** Frees every texture the layers do not show. */
    keep(layers: WindLayers) {
      const wanted = new Set(framesOf(layers).map((f) => f.path));
      for (const [path, t] of textures) {
        if (!wanted.has(path)) {
          gl.deleteTexture(t);
          textures.delete(path);
        }
      }
    },
    clear() {
      for (const t of textures.values()) gl.deleteTexture(t);
      textures.clear();
      gl.deleteTexture(empty);
    },
  };
}

export type WindTextures = ReturnType<typeof windTextures>;

/** Binds the layers to texture units `unit` … `unit + WIND_UNITS - 1` and sets the wind
 * uniforms. */
export function bindWind(
  gl: WebGL2RenderingContext,
  p: Program,
  textures: WindTextures,
  layers: WindLayers,
  unit: number,
) {
  // Every texture made first: making one binds it to the active unit, which would take another
  // layer's place there.
  const made = NAMES.map((name) => [layers[name]?.a, layers[name]?.b].map((f) => textures.get(f)));
  NAMES.forEach((name, k) => {
    const pair = layers[name];
    for (const [i, f] of [pair?.a, pair?.b].entries()) {
      bindTexture(gl, made[k][i], unit + k * 2 + i);
      gl.uniform1i(p.u[`u_${name}${i}`], unit + k * 2 + i);
      gl.uniform4f(
        p.u[`u_${name}_bounds${i}`],
        f?.meta.west ?? 0,
        f?.meta.south ?? 0,
        f?.meta.east ?? 1,
        f?.meta.north ?? 1,
      );
      gl.uniform4f(
        p.u[`u_${name}_range${i}`],
        f?.meta.uMin ?? 0,
        f?.meta.uMax ?? 0,
        f?.meta.vMin ?? 0,
        f?.meta.vMax ?? 0,
      );
    }
    gl.uniform2f(p.u[`u_${name}_gust`], pair?.a.meta.gustMax ?? -1, pair?.b.meta.gustMax ?? -1);
  });
  gl.uniform3f(p.u.u_mix, layers.base?.t ?? 0, layers.mid?.t ?? 0, layers.detail?.t ?? 0);
  gl.uniform3f(p.u.u_has, layers.base ? 1 : 0, layers.mid ? 1 : 0, layers.detail ? 1 : 0);
}
