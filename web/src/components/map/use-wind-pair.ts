import { useEffect, useMemo, useRef, useState } from 'react';
import type { MapFrame } from '@/bindings/MapFrame';
import { positionAt } from '@/lib/wind-map';
import { loadFrame, type WindFrame } from './frames';
import type { WindPair } from './wind-gl';

/** The two hours around `time`, loaded; while the next ones load, the last pair stays. When
 * playing, the hour after them loads ahead. */
export function useWindPair(frames: readonly MapFrame[], time: number | null, playing: boolean): WindPair | null {
  const pos = time === null || frames.length === 0 ? null : positionAt(frames, time);
  const a = pos ? frames[pos.i] : undefined;
  const b = pos ? frames[Math.min(pos.i + 1, frames.length - 1)] : undefined;
  const ahead = pos && playing ? frames[pos.i + 2] : undefined;
  const loaded = useRef(new Map<string, WindFrame>());
  const [, setVersion] = useState(0);
  const last = useRef<WindPair | null>(null);

  const want = [a?.path, b?.path, ahead?.path].filter((p): p is string => !!p);
  const key = want.join('|');
  // biome-ignore lint/correctness/useExhaustiveDependencies: `key` stands for `want`
  useEffect(() => {
    let live = true;
    for (const path of want) {
      if (loaded.current.has(path)) continue;
      loadFrame(path)
        .then((f) => {
          if (!live) return;
          loaded.current.set(path, f);
          // Keep what is wanted now and a few more for stepping back
          for (const k of [...loaded.current.keys()].slice(0, Math.max(0, loaded.current.size - 6))) {
            if (!want.includes(k)) loaded.current.delete(k);
          }
          setVersion((v) => v + 1);
        })
        .catch((e) => console.warn('wind map:', e));
    }
    return () => {
      live = false;
    };
  }, [key]);

  const fa = a && loaded.current.get(a.path);
  const fb = (b && loaded.current.get(b.path)) ?? fa;
  const t = fa && fb && fb !== fa ? (pos?.t ?? 0) : 0;
  const pair = useMemo(() => (fa && fb ? { a: fa, b: fb, t } : null), [fa, fb, t]);
  if (!pos) {
    // Nothing to show at this time (or no frames): nothing stays from before either.
    last.current = null;
    return null;
  }
  if (pair) last.current = pair;
  return pair ?? last.current;
}

/** While playing, moves the time on by an hour per second, from the first hour again at the
 * end. */
export function usePlayback(
  playing: boolean,
  frames: readonly MapFrame[],
  time: number | null,
  setTime: (t: number) => void,
) {
  const at = useRef(time);
  at.current = time;
  const set = useRef(setTime);
  set.current = setTime;
  const first = frames[0]?.validAt;
  const end = frames.at(-1)?.validAt;
  useEffect(() => {
    if (!playing || first === undefined || end === undefined) return;
    let frame = 0;
    let last = 0;
    const step = (now: number) => {
      frame = requestAnimationFrame(step);
      const dt = last ? Math.min(100, now - last) : 0;
      last = now;
      const next = (at.current ?? first) + dt * 3.6;
      set.current(next > end ? first : next);
    };
    frame = requestAnimationFrame(step);
    return () => cancelAnimationFrame(frame);
  }, [playing, first, end]);
}
