import { useEffect, useRef, useState } from 'react';
import { glideAt, glides } from '@/lib/glide';

/** The time the wind is drawn at: `time`, reached by a short glide when it stepped to a nearby
 * hour (lib/glide.ts), at once when it jumped further, while playing (which moves on smoothly by
 * itself) or with less motion asked for. */
export function useGlide(time: number | null, playing: boolean): number | null {
  const [shown, setShown] = useState(time);
  const at = useRef(time);
  useEffect(() => {
    const from = at.current;
    const still = window.matchMedia?.('(prefers-reduced-motion: reduce)').matches ?? false;
    if (time === null || from === null || playing || still || !glides(from, time)) {
      at.current = time;
      setShown(time);
      return;
    }
    let frame = 0;
    const start = performance.now();
    const step = (now: number) => {
      const t = glideAt(from, time, now - start);
      at.current = t;
      setShown(t);
      if (t !== time) frame = requestAnimationFrame(step);
    };
    frame = requestAnimationFrame(step);
    return () => cancelAnimationFrame(frame);
  }, [time, playing]);
  return shown;
}
