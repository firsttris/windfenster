/**
 * For `wind.spec.ts`: the map's own code in a page of its own. `window.windBoth` uploads the
 * hours given as the map does (`windTextures`, `bindWind`), runs the shaders' `wind()` at each
 * place on the GPU and reads the same places on the CPU (`windAt`), for the test to compare.
 */
import type { TextureMeta } from '@/bindings/TextureMeta';
import type { WindFrame } from '@/components/map/frames';
import { activate, bindFramebuffer, createProgram } from '@/components/map/gl';
import { QUAD_VERT, WIND } from '@/components/map/shaders';
import { windAt } from '@/components/map/wind-at';
import { bindWind, type WindLayers, windTextures } from '@/components/map/wind-gl';
import { gustOf, mercatorX, mercatorY } from '@/lib/wind-map';

export interface Hour {
  meta: TextureMeta;
  /** RGBA, rows north first */
  pixels: number[];
}

export interface Layer {
  a: Hour;
  b: Hour;
  t: number;
}

export interface Reading {
  u: number;
  v: number;
  /** The gust shown: never below the wind, the wind where no model has one */
  gust: number;
}

export interface Both {
  gpu: (Reading | null)[];
  cpu: ((Reading & { model: string }) | null)[];
}

const MAX_POINTS = 64;

const FRAG = `#version 300 es
precision highp float;
#define SAMPLE(t, uv) textureLod(t, uv, 0.0)
${WIND}
uniform vec2 u_points[${MAX_POINTS}];
out vec4 color;
void main() {
  color = wind(u_points[int(gl_FragCoord.x)]);
}`;

let made = 0;

async function frame(hour: Hour): Promise<WindFrame> {
  const { width, height } = hour.meta;
  const data = new ImageData(new Uint8ClampedArray(hour.pixels), width, height);
  // As the map decodes its PNGs (frames.ts)
  const image = await createImageBitmap(data, { premultiplyAlpha: 'none', colorSpaceConversion: 'none' });
  made += 1;
  return { path: `test/${made}`, meta: hour.meta, image };
}

async function pair(layer: Layer | null) {
  if (!layer) return null;
  const a = await frame(layer.a);
  const b = layer.b === layer.a ? a : await frame(layer.b);
  return { a, b, t: layer.t };
}

async function windBoth(
  given: { base: Layer | null; mid: Layer | null; detail: Layer | null },
  places: [number, number][],
): Promise<Both> {
  const layers: WindLayers = {
    base: await pair(given.base),
    mid: await pair(given.mid),
    detail: await pair(given.detail),
  };
  const canvas = document.createElement('canvas');
  const gl = canvas.getContext('webgl2');
  if (!gl) throw new Error('no WebGL2');
  if (!gl.getExtension('EXT_color_buffer_float')) throw new Error('no float render targets');
  const program = createProgram(gl, QUAD_VERT, FRAG);
  const textures = windTextures(gl);
  const target = gl.createTexture();
  gl.bindTexture(gl.TEXTURE_2D, target);
  gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA32F, places.length, 1, 0, gl.RGBA, gl.FLOAT, null);
  const fb = gl.createFramebuffer();
  bindFramebuffer(gl, fb, target);
  gl.viewport(0, 0, places.length, 1);
  activate(gl, program);
  bindWind(gl, program, textures, layers, 0);
  const merc = new Float32Array(MAX_POINTS * 2);
  for (const [i, [lon, lat]] of places.entries()) merc.set([mercatorX(lon), mercatorY(lat)], i * 2);
  gl.uniform2fv(program.u['u_points[0]'], merc);
  const quad = gl.createBuffer();
  gl.bindBuffer(gl.ARRAY_BUFFER, quad);
  gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([0, 0, 1, 0, 0, 1, 0, 1, 1, 0, 1, 1]), gl.STATIC_DRAW);
  gl.enableVertexAttribArray(program.a.a_pos);
  gl.vertexAttribPointer(program.a.a_pos, 2, gl.FLOAT, false, 0, 0);
  gl.drawArrays(gl.TRIANGLES, 0, 6);
  const out = new Float32Array(places.length * 4);
  gl.readPixels(0, 0, places.length, 1, gl.RGBA, gl.FLOAT, out);
  textures.clear();
  return {
    gpu: places.map((_, i) => {
      const [u, v, gust, inside] = out.subarray(i * 4, i * 4 + 4);
      return inside < 0.5 ? null : { u, v, gust };
    }),
    cpu: places.map(([lon, lat]) => {
      const here = windAt(layers, lon, lat);
      if (!here) return null;
      const { u, v } = here.wind;
      return { u, v, gust: gustOf(here.wind), model: here.model };
    }),
  };
}

declare global {
  interface Window {
    windBoth: typeof windBoth;
  }
}
window.windBoth = windBoth;
