/**
 * The moving particles, after mapbox/webgl-wind (ISC): positions live in a texture, a shader
 * moves them along the wind every frame, and each frame draws over the last one a little
 * fainter, which leaves the trails. Unlike there, positions are relative to the visible map,
 * so the particles are equally dense at every zoom; they move with the map when it pans.
 * Drawn on a canvas of their own over the map, so the map itself does not redraw 60 times a
 * second.
 */
import { countsAsFrame, nextBudget, particleCount, SAMPLE_FRAMES } from '@/lib/particle-budget';
import type { Theme } from '@/lib/prefs';
import { townness, zoomOf } from '@/lib/town-zoom';
import { trailFade } from '@/lib/trails';
import { type MercatorView, rampPixels } from '@/lib/wind-map';
import { activate, bindFramebuffer, bindTexture, createProgram, createTexture } from './gl';
import { drawParticles, fadeTrails, moveParticles, particlePasses, screenQuad } from './particle-passes';
import { COPY_FRAG, QUAD_VERT } from './shaders';
import { hasWind, NO_WIND, WIND_UNITS, type WindLayers, windTextures } from './wind-gl';

export interface Particles {
  show(layers: WindLayers): void;
  /** The visible map changed: the particles and their trails move with it. */
  setView(view: MercatorView, cssWidth: number, cssHeight: number, pixelRatio: number): void;
  /** Over the town map the particles lean towards the theme's ink, to stand out from it. */
  setTheme(theme: Theme): void;
  /** Colours by the gusts instead of the wind where the particles take the field's colours. */
  setGusts(gusts: boolean): void;
  destroy(): void;
}

/** Screen pixels per knot and frame (at 60 frames a second) */
const PX_PER_KN = 0.08;
/** Over the field the particles are white; over the town map their colour leans this far
 * towards the theme's ink: light lines on the dark map, dark ones on the light. */
const INK: Record<Theme, [number, number, number, number]> = {
  dark: [1, 1, 1, 0.45],
  light: [0.04, 0.05, 0.07, 0.2],
};

/** Share of particles renewed per frame */
const DROP = 0.004;

export function particles(canvas: HTMLCanvasElement): Particles | null {
  const context = canvas.getContext('webgl2', {
    premultipliedAlpha: true,
    alpha: true,
    antialias: false,
    depth: false,
  });
  if (!context) return null;
  const gl: WebGL2RenderingContext = context;
  const passes = particlePasses(gl);
  const { fb } = passes;
  const copy = createProgram(gl, QUAD_VERT, COPY_FRAG);
  const textures = windTextures(gl);
  const ramp = createTexture(gl, gl.LINEAR, rampPixels(), 256, 1);

  let shown: WindLayers = NO_WIND;
  let view: MercatorView = { x: 0, y: 0, w: 1, h: 1 };
  let shift: [number, number, number, number] = [1, 1, 0, 0];
  let size = { w: 1, h: 1, ratio: 1 };
  let ink = INK.dark;
  let byGust = false;
  let res = 0;
  let state: [WebGLTexture, WebGLTexture] | null = null;
  let trails: [WebGLTexture, WebGLTexture] | null = null;
  let frame = 0;
  // Frames drawn, for the update's random numbers
  let count = Math.floor(Math.random() * 1e9);
  let last = 0;
  // Fewer particles when the device cannot keep up
  let budget = 1;
  let measured = { ms: 0, frames: 0 };

  /** New particles at random places, as many as the map's size asks for */
  function seed() {
    const n = particleCount(size.w, size.h, budget);
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
      // Linear: a zoom samples them between pixels
      trails = [createTexture(gl, gl.LINEAR, null, width, height), createTexture(gl, gl.LINEAR, null, width, height)];
    }
    for (const t of trails) {
      bindFramebuffer(gl, fb, t);
      gl.clearColor(0, 0, 0, 0);
      gl.clear(gl.COLOR_BUFFER_BIT);
    }
    bindFramebuffer(gl, null);
  }

  function step(now: number) {
    frame = requestAnimationFrame(step);
    if (!state || !trails) return;
    const dt = last ? Math.min(3, (now - last) / (1000 / 60)) : 1;
    if (last && countsAsFrame(now - last) && hasWind(shown)) {
      measured = { ms: measured.ms + now - last, frames: measured.frames + 1 };
      if (measured.frames >= SAMPLE_FRAMES) {
        const next = nextBudget(budget, measured.ms / measured.frames);
        measured = { ms: 0, frames: 0 };
        if (next !== budget) {
          budget = next;
          seed();
        }
      }
    }
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
    const at = { res, view, shift };
    count = (count + 1) >>> 0;
    moveParticles(gl, passes, textures, shown, state[0], state[1], at, {
      cssWidth: size.w,
      cssHeight: size.h,
      pxPerKn: PX_PER_KN * dt,
      frame: count,
      drop: DROP * dt,
    });

    // 2. Last frame's trails, fainter and moved with the map: trails[0] → trails[1]
    fadeTrails(gl, passes, trails[0], trails[1], { width, height, fade: trailFade(dt, shift), shift });

    // 3. Each particle from where it was to where it is, over them
    drawParticles(gl, passes, textures, shown, state[0], state[1], at, {
      width,
      height,
      lineWidth: size.ratio,
      move: PX_PER_KN * dt * size.ratio,
      ink,
      ramp,
      town: townness(zoomOf(view.w, size.w)),
      gusts: byGust,
    });

    // 4. Onto the canvas
    bindFramebuffer(gl, null);
    activate(gl, copy);
    bindTexture(gl, trails[1], WIND_UNITS);
    gl.uniform1i(copy.u.u_screen_tex, WIND_UNITS);
    screenQuad(gl, passes, copy);

    state = [state[1], state[0]];
    trails = [trails[1], trails[0]];
    shift = [1, 1, 0, 0];
  }

  frame = requestAnimationFrame(step);

  return {
    show(layers) {
      shown = layers;
    },
    setView(next, cssWidth, cssHeight, pixelRatio) {
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
      // A new size starts afresh; a pan or zoom moves particles and trails along (UPDATE_FRAG,
      // FADE_FRAG), so they never flicker or vanish while the map moves.
      if (resized || !state) {
        seed();
        shift = [1, 1, 0, 0];
        clearTrails(resized);
      }
    },
    setTheme(theme) {
      ink = INK[theme];
    },
    setGusts(g) {
      byGust = g;
    },
    destroy() {
      cancelAnimationFrame(frame);
      textures.clear();
      gl.deleteTexture(ramp);
      gl.getExtension('WEBGL_lose_context')?.loseContext();
    },
  };
}
