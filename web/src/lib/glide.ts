/** The wind map's hour, glided: a step between nearby hours slides the wind there in a moment
 * instead of jumping, as the particles and the field blend any time between two model hours. */

/** How long a glide takes, ms */
export const GLIDE_MS = 320;
/** Steps up to this far glide (seconds); a jump further would load every hour on the way. */
export const GLIDE_MAX = 3 * 3600;

/** Whether going from `from` to `to` glides */
export function glides(from: number, to: number): boolean {
  return from !== to && Math.abs(to - from) <= GLIDE_MAX;
}

/** Where a glide from `from` to `to` is after `ms`: fast at first, settling softly (ease out). */
export function glideAt(from: number, to: number, ms: number): number {
  const t = Math.min(1, Math.max(0, ms / GLIDE_MS));
  return from + (to - from) * (1 - (1 - t) ** 3);
}
