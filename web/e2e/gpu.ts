/**
 * For the specs in e2e/: the map's own code in a page of its own, with hours given by the test
 * and uploaded as the map does (`windTextures`, `bindWind`).
 * - `window.windBoth` runs the shaders' `wind()` at each place on the GPU and reads the same
 *   places on the CPU (`windAt`), for `wind.spec.ts` to compare.
 * - `window.particleRun` moves particles from known places through the update pass a few times
 *   and draws the last step, for `particles.spec.ts`.
 * - `window.fadeRun` fades a few lit pixels once, moved as after a pan or zoom.
 * - `window.scene` paints the colour field as the map does (`windField`) and particles with their
 *   trails over it, from fixed random numbers, for `look.spec.ts` to compare with a picture.
 */
import type { TextureMeta } from '@/bindings/TextureMeta';
import type { WindFrame } from '@/components/map/frames';
import { activate, bindFramebuffer, bindTexture, createProgram, createTexture } from '@/components/map/gl';
import { drawParticles, fadeTrails, moveParticles, particlePasses, screenQuad } from '@/components/map/particle-passes';
import { COPY_FRAG, PRECISION, QUAD_VERT, WIND } from '@/components/map/shaders';
import { windAt } from '@/components/map/wind-at';
import { windField } from '@/components/map/wind-field';
import { bindWind, WIND_UNITS, type WindLayers, windTextures } from '@/components/map/wind-gl';
import { particleCount } from '@/lib/particle-budget';
import { FADE, PX_PER_KN } from '@/lib/trails';
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
${PRECISION}
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
  /** The map moved before the first step: new = old * xy + zw (identity when not given) */
  shift?: [number, number, number, number];
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
  const at = { res: run.res, view: run.view, shift: run.shift ?? ([1, 1, 0, 0] as [number, number, number, number]) };
  let from = createTexture(gl, gl.NEAREST, encode(run.start), run.res, run.res);
  let previous = from;
  const states: number[][] = [];
  for (const [k, step] of run.steps.entries()) {
    const to = createTexture(gl, gl.NEAREST, null, run.res, run.res);
    moveParticles(gl, passes, textures, layers, from, to, at, {
      cssWidth: run.cssWidth,
      cssHeight: run.cssHeight,
      ...step,
    });
    const bytes = new Uint8Array(run.res * run.res * 4);
    gl.readPixels(0, 0, run.res, run.res, gl.RGBA, gl.UNSIGNED_BYTE, bytes);
    states.push(decode(bytes));
    // The move counts for the first step; the draw after the last step sees it too
    if (k < run.steps.length - 1) at.shift = [1, 1, 0, 0];
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

/** Random numbers from a seed (mulberry32), so a scene is the same every time */
function random(seed: number) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

export interface Scene {
  width: number;
  height: number;
  /** West, south, east, north */
  bounds: [number, number, number, number];
  gusts: boolean;
  /** Particle frames before the picture, at 60 a second; 0 for the field alone */
  frames: number;
  /** Each frame's length in frames at 60 a second, as particles.ts takes it: 0.42 at 144 Hz;
   * 1 when not given */
  dt?: number;
  /** The map's zoom the field is told (it fades out at town zoom); 5 when not given */
  zoom?: number;
  /** The field's opacity; 0.9 as on the map when not given */
  opacity?: number;
  /** A transparent background instead of the map's dark one */
  clear?: boolean;
}

/** Paints a scene onto a canvas `#scene`: the colour field, then particles with their trails. */
async function scene(given: { base: Layer | null; mid: Layer | null; detail: Layer | null }, sc: Scene): Promise<void> {
  const layers = await layersOf(given);
  document.getElementById('scene')?.remove();
  const canvas = document.createElement('canvas');
  canvas.id = 'scene';
  canvas.width = sc.width;
  canvas.height = sc.height;
  canvas.style.cssText = `display:block;width:${sc.width}px;height:${sc.height}px;background:#0b0d10`;
  document.body.style.margin = '0';
  document.body.append(canvas);
  const gl = canvas.getContext('webgl2', { premultipliedAlpha: true, preserveDrawingBuffer: true, antialias: false });
  if (!gl) throw new Error('no WebGL2');
  const [west, south, east, north] = sc.bounds;
  const view = {
    x: mercatorX(west),
    y: mercatorY(north),
    w: mercatorX(east) - mercatorX(west),
    h: mercatorY(south) - mercatorY(north),
  };
  if (sc.clear) gl.clearColor(0, 0, 0, 0);
  else gl.clearColor(0.043, 0.051, 0.063, 1);
  gl.clear(gl.COLOR_BUFFER_BIT);

  // The field as the map paints it, through a stand-in for MapLibre
  const field = windField('field', sc.opacity ?? 0.9, sc.gusts);
  const map = { getZoom: () => sc.zoom ?? 5, triggerRepaint: () => {} };
  field.layer.onAdd?.(map as never, gl);
  field.show(layers);
  const [sx, sy] = [2 / view.w, -2 / view.h];
  const matrix = [sx, 0, 0, 0, 0, sy, 0, 0, 0, 0, 1, 0, -1 - view.x * sx, 1 - view.y * sy, 0, 1];
  field.layer.render?.(gl, { defaultProjectionData: { mainMatrix: matrix } } as never);

  if (sc.frames === 0) {
    gl.finish();
    return;
  }
  // Particles as particles.ts runs them: move, fade the trails, draw, for `frames` frames
  const passes = particlePasses(gl);
  const copy = createProgram(gl, QUAD_VERT, COPY_FRAG);
  const textures = windTextures(gl);
  const ramp = createTexture(gl, gl.LINEAR, rampPixels(), 256, 1);
  // As many particles as the map has on a view this size
  const res = Math.ceil(Math.sqrt(particleCount(sc.width, sc.height, 1)));
  const next = random(42);
  const seed = Uint8Array.from({ length: res * res * 4 }, () => Math.floor(next() * 256));
  let state = [createTexture(gl, gl.NEAREST, seed, res, res), createTexture(gl, gl.NEAREST, seed, res, res)];
  let trails = [0, 1].map(() => createTexture(gl, gl.LINEAR, null, sc.width, sc.height));
  for (const t of trails) {
    bindFramebuffer(gl, passes.fb, t);
    gl.clearColor(0, 0, 0, 0);
    gl.clear(gl.COLOR_BUFFER_BIT);
  }
  const at = { res, view, shift: [1, 1, 0, 0] as [number, number, number, number] };
  const dt = sc.dt ?? 1;
  for (let frame = 1; frame <= sc.frames; frame++) {
    gl.disable(gl.BLEND);
    moveParticles(gl, passes, textures, layers, state[0], state[1], at, {
      cssWidth: sc.width,
      cssHeight: sc.height,
      pxPerKn: PX_PER_KN * dt,
      frame,
      drop: 0.004 * dt,
    });
    fadeTrails(gl, passes, trails[0], trails[1], {
      width: sc.width,
      height: sc.height,
      fade: FADE ** dt,
      shift: at.shift,
    });
    drawParticles(gl, passes, textures, layers, state[0], state[1], at, {
      width: sc.width,
      height: sc.height,
      lineWidth: 1,
      move: PX_PER_KN * dt,
      ink: [1, 1, 1, 0.45],
      ramp,
      town: 0,
      gusts: sc.gusts,
    });
    state = [state[1], state[0]];
    trails = [trails[1], trails[0]];
  }
  // The trails over the field, as the particles' canvas lies over the map
  bindFramebuffer(gl, null);
  gl.viewport(0, 0, sc.width, sc.height);
  gl.enable(gl.BLEND);
  gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
  activate(gl, copy);
  bindTexture(gl, trails[0], WIND_UNITS);
  gl.uniform1i(copy.u.u_screen_tex, WIND_UNITS);
  screenQuad(gl, passes, copy);
  gl.finish();
}

export interface FadeRun {
  width: number;
  height: number;
  /** Pixels lit white before, (x, y) from the top left */
  lit: [number, number][];
  fade: number;
  shift: [number, number, number, number];
}

/** The trails' fade pass once over a target with a few pixels lit: its alpha after, row by row
 * from the top, base64 */
function fadeRun(run: FadeRun): string {
  const canvas = document.createElement('canvas');
  const gl = canvas.getContext('webgl2');
  if (!gl) throw new Error('no WebGL2');
  const passes = particlePasses(gl);
  const { width, height } = run;
  const before = new Uint8Array(width * height * 4);
  // Rows from the bottom, as the trails are drawn
  for (const [x, y] of run.lit)
    before.fill(255, ((height - 1 - y) * width + x) * 4, ((height - 1 - y) * width + x) * 4 + 4);
  const from = createTexture(gl, gl.LINEAR, before, width, height);
  const to = createTexture(gl, gl.LINEAR, null, width, height);
  fadeTrails(gl, passes, from, to, run);
  const rgba = new Uint8Array(width * height * 4);
  gl.readPixels(0, 0, width, height, gl.RGBA, gl.UNSIGNED_BYTE, rgba);
  const alpha = new Uint8Array(width * height);
  for (let y = 0; y < height; y++)
    for (let x = 0; x < width; x++) alpha[y * width + x] = rgba[((height - 1 - y) * width + x) * 4 + 3];
  let text = '';
  for (let i = 0; i < alpha.length; i += 0x8000) text += String.fromCharCode(...alpha.subarray(i, i + 0x8000));
  return btoa(text);
}

/** The scene's pixels at (x, y) from the top left, RGBA as stored (premultiplied) */
function pixelsAt(places: [number, number][]): number[][] {
  const canvas = document.getElementById('scene') as HTMLCanvasElement;
  const gl = canvas.getContext('webgl2') as WebGL2RenderingContext;
  return places.map(([x, y]) => {
    const px = new Uint8Array(4);
    gl.readPixels(x, canvas.height - 1 - y, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, px);
    return [...px];
  });
}

declare global {
  interface Window {
    windBoth: typeof windBoth;
    particleRun: typeof particleRun;
    scene: typeof scene;
    pixelsAt: typeof pixelsAt;
    fadeRun: typeof fadeRun;
  }
}
window.pixelsAt = pixelsAt;
window.windBoth = windBoth;
window.particleRun = particleRun;
window.scene = scene;
window.fadeRun = fadeRun;
