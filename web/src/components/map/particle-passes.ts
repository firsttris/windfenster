/**
 * The particles' two passes on the GPU: moving them along the wind (state → state) and drawing
 * each from where it was to where it is. `particles.ts` runs them every frame; the browser tests
 * (`e2e/particles.spec.ts`) run them with positions and wind of their own.
 */
import { type MercatorView, RAMP_MAX_KN } from '@/lib/wind-map';
import { activate, bindAttribute, bindFramebuffer, bindTexture, createBuffer, createProgram, type Program } from './gl';
import { DRAW_FRAG, DRAW_VERT, FADE_FRAG, QUAD_VERT, UPDATE_FRAG } from './shaders';
import { bindWind, WIND_UNITS, type WindLayers, type WindTextures } from './wind-gl';

export interface ParticlePasses {
  update: Program;
  draw: Program;
  fade: Program;
  /** Two triangles over the whole target */
  quad: WebGLBuffer;
  /** A particle's quad: tail (x 0) to head (x 1), both sides (y ±1) */
  corners: WebGLBuffer;
  fb: WebGLFramebuffer;
}

export function particlePasses(gl: WebGL2RenderingContext): ParticlePasses {
  return {
    update: createProgram(gl, QUAD_VERT, UPDATE_FRAG),
    fade: createProgram(gl, QUAD_VERT, FADE_FRAG),
    draw: createProgram(gl, DRAW_VERT, DRAW_FRAG),
    quad: createBuffer(gl, new Float32Array([0, 0, 1, 0, 0, 1, 0, 1, 1, 0, 1, 1])),
    corners: createBuffer(gl, new Float32Array([0, -1, 1, -1, 0, 1, 0, 1, 1, -1, 1, 1])),
    fb: gl.createFramebuffer(),
  };
}

export function screenQuad(gl: WebGL2RenderingContext, passes: ParticlePasses, p: Program) {
  bindAttribute(gl, passes.quad, p.a.a_pos, 2);
  gl.drawArrays(gl.TRIANGLES, 0, 6);
}

/** Where the particles are and what the screen shows */
export interface ParticleView {
  /** The particle state is `res` × `res` texels, one particle each */
  res: number;
  /** The visible map in Mercator */
  view: MercatorView;
  /** The view moved since the last frame: new position = old * xy + zw */
  shift: [number, number, number, number];
}

/** Moves every particle in `from` along the wind into `to`: `pxPerKn` CSS pixels per knot on a
 * map `cssWidth` × `cssHeight`; `drop` of them (more in strong wind) start anew at random, as do
 * those leaving the wind or the view. `frame` seeds the random numbers. */
export function moveParticles(
  gl: WebGL2RenderingContext,
  passes: ParticlePasses,
  textures: WindTextures,
  layers: WindLayers,
  from: WebGLTexture,
  to: WebGLTexture,
  at: ParticleView,
  o: { cssWidth: number; cssHeight: number; pxPerKn: number; frame: number; drop: number },
) {
  const { update } = passes;
  bindFramebuffer(gl, passes.fb, to);
  gl.viewport(0, 0, at.res, at.res);
  activate(gl, update);
  bindWind(gl, update, textures, layers, 0);
  bindTexture(gl, from, WIND_UNITS);
  gl.uniform1i(update.u.u_particles, WIND_UNITS);
  gl.uniform4f(update.u.u_view, at.view.x, at.view.y, at.view.w, at.view.h);
  gl.uniform4f(update.u.u_shift, ...at.shift);
  gl.uniform2f(update.u.u_screen, o.cssWidth, o.cssHeight);
  gl.uniform1f(update.u.u_px_per_kn, o.pxPerKn);
  gl.uniform1ui(update.u.u_frame, o.frame);
  gl.uniform1f(update.u.u_drop, o.drop);
  screenQuad(gl, passes, update);
}

/** Last frame's trails `from` into `to` (`width` × `height` device pixels), each pixel `fade`
 * as bright, moved with the map by `shift`. */
export function fadeTrails(
  gl: WebGL2RenderingContext,
  passes: ParticlePasses,
  from: WebGLTexture,
  to: WebGLTexture,
  o: { width: number; height: number; fade: number; shift: [number, number, number, number] },
) {
  const { fade } = passes;
  bindFramebuffer(gl, passes.fb, to);
  gl.viewport(0, 0, o.width, o.height);
  activate(gl, fade);
  bindTexture(gl, from, WIND_UNITS);
  gl.uniform1i(fade.u.u_screen_tex, WIND_UNITS);
  gl.uniform1f(fade.u.u_fade, o.fade);
  gl.uniform4f(fade.u.u_shift, ...o.shift);
  screenQuad(gl, passes, fade);
}

/** Draws each particle from `before` to `after` into what is bound, `width` × `height` device
 * pixels, over what is there: lines `lineWidth` wide, a particle that started anew without a
 * tail (`move`: device pixels per knot this frame). */
export function drawParticles(
  gl: WebGL2RenderingContext,
  passes: ParticlePasses,
  textures: WindTextures,
  layers: WindLayers,
  before: WebGLTexture,
  after: WebGLTexture,
  at: ParticleView,
  o: {
    width: number;
    height: number;
    lineWidth: number;
    move: number;
    ink: [number, number, number, number];
    ramp: WebGLTexture;
    town: number;
    gusts: boolean;
  },
) {
  const { draw } = passes;
  gl.enable(gl.BLEND);
  gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
  activate(gl, draw);
  bindWind(gl, draw, textures, layers, 0);
  bindTexture(gl, before, WIND_UNITS);
  bindTexture(gl, after, WIND_UNITS + 1);
  gl.uniform1i(draw.u.u_before, WIND_UNITS);
  gl.uniform1i(draw.u.u_after, WIND_UNITS + 1);
  gl.uniform4f(draw.u.u_shift, ...at.shift);
  gl.uniform4f(draw.u.u_view, at.view.x, at.view.y, at.view.w, at.view.h);
  gl.uniform2f(draw.u.u_screen, o.width, o.height);
  gl.uniform1f(draw.u.u_res, at.res);
  gl.uniform1f(draw.u.u_width, o.lineWidth);
  gl.uniform1f(draw.u.u_move, o.move);
  gl.uniform4f(draw.u.u_ink, ...o.ink);
  bindTexture(gl, o.ramp, WIND_UNITS + 2);
  gl.uniform1i(draw.u.u_ramp, WIND_UNITS + 2);
  gl.uniform1f(draw.u.u_ramp_max, RAMP_MAX_KN);
  gl.uniform1f(draw.u.u_town, o.town);
  gl.uniform1f(draw.u.u_gusts, o.gusts ? 1 : 0);
  bindAttribute(gl, passes.corners, draw.a.a_corner, 2);
  gl.drawArraysInstanced(gl.TRIANGLES, 0, 6, at.res * at.res);
  gl.disable(gl.BLEND);
}
