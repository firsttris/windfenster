/**
 * For the specs in e2e/: the map's own code in a page of its own, with hours given by the test
 * and uploaded as the map does (`windTextures`, `bindWind`).
 * - `window.windBoth` runs the shaders' `wind()` at each place on the GPU and reads the same
 *   places on the CPU (`windAt`), for `wind.spec.ts` to compare.
 * - `window.particleRun` moves particles from known places through the update pass a few times
 *   and draws the last step, for `particles.spec.ts`.
 */
import type { TextureMeta } from '@/bindings/TextureMeta';
import type { WindFrame } from '@/components/map/frames';
import { activate, bindFramebuffer, createProgram, createTexture } from '@/components/map/gl';
import { drawParticles, moveParticles, particlePasses } from '@/components/map/particle-passes';
import { QUAD_VERT, WIND } from '@/components/map/shaders';
import { windAt } from '@/components/map/wind-at';
import { bindWind, type WindLayers, windTextures } from '@/components/map/wind-gl';
import { gustOf, type MercatorView, mercatorX, mercatorY, rampPixels } from '@/lib/wind-map';

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

async function layersOf(given: { base: Layer | null; mid: Layer | null; detail: Layer | null }): Promise<WindLayers> {
  return { base: await pair(given.base), mid: await pair(given.mid), detail: await pair(given.detail) };
}

/** A particle state texture's bytes for places 0…1 from the top left, as the shaders encode them
 * (two bytes per axis: red and blue for x, green and alpha for y). */
function encode(places: number[]): Uint8Array {
  const bytes = new Uint8Array(places.length * 2);
  for (let i = 0; i < places.length / 2; i++) {
    for (const [k, v] of [places[i * 2], places[i * 2 + 1]].entries()) {
      const s = Math.min(255, Math.max(0, v * 255));
      const hi = Math.min(255, Math.floor(s));
      bytes[i * 4 + k] = Math.round((s - hi) * 255);
      bytes[i * 4 + 2 + k] = hi;
    }
  }
  return bytes;
}

function decode(bytes: Uint8Array): number[] {
  const out: number[] = [];
  for (let i = 0; i < bytes.length; i += 4)
    out.push(bytes[i] / 255 / 255 + bytes[i + 2] / 255, bytes[i + 1] / 255 / 255 + bytes[i + 3] / 255);
  return out;
}

export interface ParticleRun {
  /** Places (x, y, x, y, …, 0…1 from the top left), `res` × `res` of them */
  start: number[];
  res: number;
  view: MercatorView;
  cssWidth: number;
  cssHeight: number;
  /** Update passes, one after the other */
  steps: { pxPerKn: number; drop: number; frame: number }[];
  /** Draws the last step into a target of this many device pixels */
  draw?: { width: number; height: number; lineWidth: number; move: number };
}

export interface ParticleResult {
  /** Places after each step */
  states: number[][];
  /** The drawn target's alpha, row by row from the bottom, base64 */
  alpha?: string;
}

async function particleRun(
  given: { base: Layer | null; mid: Layer | null; detail: Layer | null },
  run: ParticleRun,
): Promise<ParticleResult> {
  const layers = await layersOf(given);
  const canvas = document.createElement('canvas');
  const gl = canvas.getContext('webgl2');
  if (!gl) throw new Error('no WebGL2');
  const passes = particlePasses(gl);
  const textures = windTextures(gl);
  const at = { res: run.res, view: run.view, shift: [1, 1, 0, 0] as [number, number, number, number] };
  let from = createTexture(gl, gl.NEAREST, encode(run.start), run.res, run.res);
  let previous = from;
  const states: number[][] = [];
  for (const step of run.steps) {
    const to = createTexture(gl, gl.NEAREST, null, run.res, run.res);
    moveParticles(gl, passes, textures, layers, from, to, at, {
      cssWidth: run.cssWidth,
      cssHeight: run.cssHeight,
      ...step,
    });
    const bytes = new Uint8Array(run.res * run.res * 4);
    gl.readPixels(0, 0, run.res, run.res, gl.RGBA, gl.UNSIGNED_BYTE, bytes);
    states.push(decode(bytes));
    previous = from;
    from = to;
  }
  if (!run.draw) return { states };
  const { width, height } = run.draw;
  const target = createTexture(gl, gl.NEAREST, null, width, height);
  bindFramebuffer(gl, passes.fb, target);
  gl.viewport(0, 0, width, height);
  gl.clearColor(0, 0, 0, 0);
  gl.clear(gl.COLOR_BUFFER_BIT);
  const ramp = createTexture(gl, gl.LINEAR, rampPixels(), 256, 1);
  drawParticles(gl, passes, textures, layers, previous, from, at, {
    ...run.draw,
    ink: [1, 1, 1, 0.45],
    ramp,
    town: 0,
    gusts: false,
  });
  const rgba = new Uint8Array(width * height * 4);
  gl.readPixels(0, 0, width, height, gl.RGBA, gl.UNSIGNED_BYTE, rgba);
  const alpha = new Uint8Array(width * height);
  for (let i = 0; i < alpha.length; i++) alpha[i] = rgba[i * 4 + 3];
  let text = '';
  for (let i = 0; i < alpha.length; i += 0x8000) text += String.fromCharCode(...alpha.subarray(i, i + 0x8000));
  return { states, alpha: btoa(text) };
}

declare global {
  interface Window {
    windBoth: typeof windBoth;
    particleRun: typeof particleRun;
  }
}
window.windBoth = windBoth;
window.particleRun = particleRun;
