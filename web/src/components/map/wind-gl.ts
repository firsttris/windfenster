/** What both wind layers share on the GPU: the two hours on show as textures and the uniforms
 * the `wind()` GLSL function reads. */
import type { WindFrame } from './frames';
import { bindTexture, createTexture, type Program } from './gl';

/** Two hours and how far between them (0…1). */
export interface WindPair {
  a: WindFrame;
  b: WindFrame;
  t: number;
}

/** Textures per frame in one GL context, uploaded when first drawn; with `mipmaps` for a layer
 * that shows the field smaller than its texels. */
export function windTextures(gl: WebGL2RenderingContext, mipmaps = false) {
  const textures = new Map<string, WebGLTexture>();
  return {
    get(frame: WindFrame): WebGLTexture {
      let t = textures.get(frame.path);
      if (!t) {
        t = createTexture(gl, gl.LINEAR, frame.image);
        if (mipmaps) {
          gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR_MIPMAP_LINEAR);
          gl.generateMipmap(gl.TEXTURE_2D);
        }
        textures.set(frame.path, t);
      }
      return t;
    },
    /** Frees every texture but those of `pair`. */
    keep(pair: WindPair) {
      for (const [path, t] of textures) {
        if (path !== pair.a.path && path !== pair.b.path) {
          gl.deleteTexture(t);
          textures.delete(path);
        }
      }
    },
    clear() {
      for (const t of textures.values()) gl.deleteTexture(t);
      textures.clear();
    },
  };
}

export type WindTextures = ReturnType<typeof windTextures>;

/** Binds the pair to texture units `unit` and `unit + 1` and sets the wind uniforms. */
export function bindWind(gl: WebGL2RenderingContext, p: Program, textures: WindTextures, pair: WindPair, unit: number) {
  bindTexture(gl, textures.get(pair.a), unit);
  bindTexture(gl, textures.get(pair.b), unit + 1);
  gl.uniform1i(p.u.u_wind0, unit);
  gl.uniform1i(p.u.u_wind1, unit + 1);
  for (const [k, f] of [
    ['0', pair.a.meta],
    ['1', pair.b.meta],
  ] as const) {
    gl.uniform4f(p.u[`u_bounds${k}`], f.west, f.south, f.east, f.north);
    gl.uniform4f(p.u[`u_range${k}`], f.uMin, f.uMax, f.vMin, f.vMax);
  }
  gl.uniform1f(p.u.u_mix, pair.t);
}
