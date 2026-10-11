/** The timeline's arithmetic: days as segments, jumping by days, the hour under the pointer. */

export interface DaySegment {
  /** Start and end within the timeline, Unix seconds */
  from: number;
  to: number;
  /** Local midnight that starts the day */
  day: number;
}

/** The local days the span touches, each cut to the span. */
export function daySegments(from: number, to: number): DaySegment[] {
  const out: DaySegment[] = [];
  const d = new Date(from * 1000);
  d.setHours(0, 0, 0, 0);
  while (d.getTime() / 1000 < to) {
    const day = d.getTime() / 1000;
    d.setDate(d.getDate() + 1);
    const end = d.getTime() / 1000;
    out.push({ from: Math.max(day, from), to: Math.min(end, to), day });
  }
  return out;
}

/** The same hour of another day, kept within the span. */
export function onDay(time: number, day: number, from: number, to: number): number {
  const t = new Date(time * 1000);
  const d = new Date(day * 1000);
  d.setHours(t.getHours(), 0, 0, 0);
  return clamp(d.getTime() / 1000, from, to);
}

/** The full hour at a fraction (0…1) of the span. */
export function hourAt(fraction: number, from: number, to: number): number {
  const t = from + Math.min(1, Math.max(0, fraction)) * (to - from);
  return clamp(Math.round(t / 3600) * 3600, from, to);
}

export function clamp(t: number, from: number, to: number): number {
  return Math.min(to, Math.max(from, t));
}
