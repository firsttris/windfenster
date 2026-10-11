/**
 * The moving particles, after mapbox/webgl-wind (ISC): positions live in a texture, a shader
 * moves them along the wind every frame, and each frame draws over the last one a little
 * fainter, which leaves the trails. Unlike there, positions are relative to the visible map,
 * so the particles are equally dense at every zoom; they move with the map when it pans.
 * Drawn on a canvas of their own over the map, so the map itself does not redraw 60 times a
 * second.
 */
import type { MercatorView } from '@/lib/wind-map';
import {
  activate,
  bindAttribute,
  bindFramebuffer,
  bindTexture,
  createBuffer,
  createProgram,
  createTexture,
  type Program,
} from './gl';
import { COPY_FRAG, DRAW_FRAG, DRAW_VERT, FADE_FRAG, QUAD_VERT, UPDATE_FRAG } from './shaders';
import { bindWind, hasWind, NO_WIND, type WindLayers, windTextures } from './wind-gl';

export interface Particles {
  show(layers: WindLayers): void;
  /** The visible map changed; `zoomed` starts the particles afresh. */
  setView(view: MercatorView, cssWidth: number, cssHeight: number, pixelRatio: number, zoomed: boolean): void;
  setColor(rgb: [number, number, number]): void;
  destroy(): void;
}

/** Screen pixels per knot and frame (at 60 frames a second) */
const PX_PER_KN = 0.08;
/** How much of a trail is left after one frame */
const FADE = 0.97;
/** Share of particles renewed per frame */
const DROP = 0.004;
/** One particle per this many square pixels of map, within limits */
const AREA_PER_PARTICLE = 480;

export function particles(canvas: HTMLCanvasElement): Particles | null {
  const context = canvas.getContext('webgl2', {
    premultipliedAlpha: true,
    alpha: true,
    antialias: false,
    depth: false,
  });
  if (!context) return null;
  const gl: WebGL2RenderingContext = context;
  const update = createProgram(gl, QUAD_VERT, UPDATE_FRAG);
  const draw = createProgram(gl, DRAW_VERT, DRAW_FRAG);
  const fade = createProgram(gl, QUAD_VERT, FADE_FRAG);
  const copy = createProgram(gl, QUAD_VERT, COPY_FRAG);
  const quad = createBuffer(gl, new Float32Array([0, 0, 1, 0, 0, 1, 0, 1, 1, 0, 1, 1]));
  // A particle's quad: tail (x 0) to head (x 1), both sides (y ±1)
  const corners = createBuffer(gl, new Float32Array([0, -1, 1, -1, 0, 1, 0, 1, 1, -1, 1, 1]));
  const fb = gl.createFramebuffer();
  const textures = windTextures(gl);

  let shown: WindLayers = NO_WIND;
  let view: MercatorView = { x: 0, y: 0, w: 1, h: 1 };
  let shift: [number, number, number, number] = [1, 1, 0, 0];
  let size = { w: 1, h: 1, ratio: 1 };
  let color: [number, number, number] = [1, 1, 1];
  let res = 0;
  let state: [WebGLTexture, WebGLTexture] | null = null;
  let trails: [WebGLTexture, WebGLTexture] | null = null;
  let frame = 0;
  let last = 0;

  /** New particles at random places, as many as the map's size asks for */
  function seed() {
    const n = Math.min(16_000, Math.max(1_500, Math.round((size.w * size.h) / AREA_PER_PARTICLE)));
    res = Math.ceil(Math.sqrt(n));
    const bytes = new Uint8Array(res * res * 4);
    for (let i = 0; i < bytes.length; i++) bytes[i] = Math.floor(Math.random() * 256);
    if (state) for (const t of state) gl.deleteTexture(t);
    state = [createTexture(gl, gl.NEAREST, bytes, res, res), createTexture(gl, gl.NEAREST, bytes, res, res)];
  }

  function clearTrails(resized: boolean) {
    const { width, height } = canvas;
    if (resized || !trails) {
      if (trails) for (const t of trails) gl.deleteTexture(t);
      trails = [createTexture(gl, gl.NEAREST, null, width, height), createTexture(gl, gl.NEAREST, null, width, height)];
    }
    for (const t of trails) {
      bindFramebuffer(gl, fb, t);
      gl.clearColor(0, 0, 0, 0);
      gl.clear(gl.COLOR_BUFFER_BIT);
    }
    bindFramebuffer(gl, null);
  }

  function screenQuad(p: Program) {
    bindAttribute(gl, quad, p.a.a_pos, 2);
    gl.drawArrays(gl.TRIANGLES, 0, 6);
  }

  function step(now: number) {
    frame = requestAnimationFrame(step);
    if (!state || !trails) return;
    const dt = last ? Math.min(3, (now - last) / (1000 / 60)) : 1;
    last = now;
    const { width, height } = canvas;
    if (!hasWind(shown)) {
      gl.clearColor(0, 0, 0, 0);
      gl.clear(gl.COLOR_BUFFER_BIT);
      return;
    }
    textures.keep(shown);
    gl.disable(gl.DEPTH_TEST);
    gl.disable(gl.BLEND);

    // 1. Move the particles: state[0] → state[1]
    bindFramebuffer(gl, fb, state[1]);
    gl.viewport(0, 0, res, res);
    activate(gl, update);
    bindWind(gl, update, textures, shown, 0);
    bindTexture(gl, state[0], 4);
    gl.uniform1i(update.u.u_particles, 4);
    gl.uniform4f(update.u.u_view, view.x, view.y, view.w, view.h);
    gl.uniform4f(update.u.u_shift, ...shift);
    gl.uniform2f(update.u.u_screen, size.w, size.h);
    gl.uniform1f(update.u.u_px_per_kn, PX_PER_KN * dt);
    gl.uniform1f(update.u.u_seed, Math.random());
    gl.uniform1f(update.u.u_drop, DROP * dt);
    screenQuad(update);

    // 2. Last frame's trails, fainter: trails[0] → trails[1]
    bindFramebuffer(gl, fb, trails[1]);
    gl.viewport(0, 0, width, height);
    activate(gl, fade);
    bindTexture(gl, trails[0], 4);
    gl.uniform1i(fade.u.u_screen_tex, 4);
    gl.uniform1f(fade.u.u_fade, FADE ** dt);
    screenQuad(fade);

    // 3. Each particle from where it was to where it is, over them
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
    activate(gl, draw);
    bindWind(gl, draw, textures, shown, 0);
    bindTexture(gl, state[0], 4);
    bindTexture(gl, state[1], 5);
    gl.uniform1i(draw.u.u_before, 4);
    gl.uniform1i(draw.u.u_after, 5);
    gl.uniform4f(draw.u.u_shift, ...shift);
    gl.uniform4f(draw.u.u_view, view.x, view.y, view.w, view.h);
    gl.uniform2f(draw.u.u_screen, width, height);
    gl.uniform1f(draw.u.u_res, res);
    gl.uniform1f(draw.u.u_width, size.ratio);
    gl.uniform3f(draw.u.u_color, ...color);
    bindAttribute(gl, corners, draw.a.a_corner, 2);
    gl.drawArraysInstanced(gl.TRIANGLES, 0, 6, res * res);
    gl.disable(gl.BLEND);

    // 4. Onto the canvas
    bindFramebuffer(gl, null);
    activate(gl, copy);
    bindTexture(gl, trails[1], 4);
    gl.uniform1i(copy.u.u_screen_tex, 4);
    screenQuad(copy);

    state = [state[1], state[0]];
    trails = [trails[1], trails[0]];
    shift = [1, 1, 0, 0];
  }

  frame = requestAnimationFrame(step);

  return {
    show(layers) {
      shown = layers;
    },
    setView(next, cssWidth, cssHeight, pixelRatio, zoomed) {
      const width = Math.round(cssWidth * pixelRatio);
      const height = Math.round(cssHeight * pixelRatio);
      const resized = width !== canvas.width || height !== canvas.height;
      if (resized) {
        canvas.width = width;
        canvas.height = height;
      }
      // Where the old view's corners lie in the new one: positions move with the map.
      const sx = view.w / next.w;
      const sy = view.h / next.h;
      shift = [
        shift[0] * sx,
        shift[1] * sy,
        shift[2] * sx + (view.x - next.x) / next.w,
        shift[3] * sy + (view.y - next.y) / next.h,
      ];
      view = next;
      size = { w: cssWidth, h: cssHeight, ratio: pixelRatio };
      if (zoomed || resized || !state) {
        seed();
        shift = [1, 1, 0, 0];
      }
      // Trails are drawn on the screen: after a move they would point the wrong way.
      clearTrails(resized);
    },
    setColor(rgb) {
      color = rgb;
    },
    destroy() {
      cancelAnimationFrame(frame);
      textures.clear();
      gl.getExtension('WEBGL_lose_context')?.loseContext();
    },
  };
}
