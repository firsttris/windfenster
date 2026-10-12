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

/** The shortest time between two steps of the particles, in ms: below a 60 Hz frame (16.7 ms)
 * with room for the browser's jitter. The trails lose a byte of alpha per step whatever the
 * time between, so on a 120 or 144 Hz screen they were half as long, and the GPU did twice the
 * work for nothing. */
export const MIN_STEP_MS = 13;

/** Whether a screen frame at `now` takes a step, the last one having been at `last` (0: none). */
export function takesStep(last: number, now: number): boolean {
  return !last || now - last >= MIN_STEP_MS;
}
