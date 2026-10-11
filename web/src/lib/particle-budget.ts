/** How many particles the map can afford: one per so many square pixels, fewer when the device
 * cannot keep up. */

/** One particle per this many square CSS pixels of map at full budget */
export const AREA_PER_PARTICLE = 480;
const MIN = 1_500;
const MAX = 16_000;

/** Frames slower than this on average (below about 45 a second) cost particles. */
export const SLOW_MS = 22;
/** Frames measured before deciding */
export const SAMPLE_FRAMES = 120;
const SHRINK = 0.7;
const FLOOR = 0.35;

export function particleCount(cssWidth: number, cssHeight: number, budget: number): number {
  return Math.min(MAX, Math.max(MIN, Math.round(((cssWidth * cssHeight) / AREA_PER_PARTICLE) * budget)));
}

/** The budget after frames took `avgMs` on average: smaller when slow, never below the floor.
 * It does not grow back: a device that was slow once stays calm. */
export function nextBudget(budget: number, avgMs: number): number {
  return avgMs > SLOW_MS ? Math.max(FLOOR, budget * SHRINK) : budget;
}

/** Whether a frame interval tells anything: hidden tabs and the first frame do not. */
export function countsAsFrame(ms: number): boolean {
  return ms > 4 && ms < 100;
}
