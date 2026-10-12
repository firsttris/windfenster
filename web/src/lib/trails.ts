/** How fast the particles run and how long their trails last. */

/** Screen pixels per knot (of the shaders' `pace`) and frame at 60 frames a second */
export const PX_PER_KN = 0.08;

/** How much of a trail is left after one frame at 60 a second */
export const FADE = 0.98;

/** How much of a trail is left after a frame `dt` frames long in which the map moved by `shift`
 * (new = old * xy + zw): while zooming the trails stretch with the map, so they fade faster the
 * more the zoom changed, at most twice as fast. A pan moves them unchanged. */
export function trailFade(dt: number, shift: readonly [number, number, number, number]): number {
  const zoom = Math.abs(Math.log2(shift[0]));
  return FADE ** dt * (1 - Math.min(0.5, zoom * 4));
}
