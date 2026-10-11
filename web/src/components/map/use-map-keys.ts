/** The map's keys on desktop: Space plays, ←/→ step an hour (with Shift a day), G switches
 * wind and gusts, N goes to now, Escape closes the spot. Keys typed into a field, on a button
 * (Space) or on the focused map itself (which pans with the arrows) are left alone. */
import { useEffect, useRef } from 'react';

interface Keys {
  togglePlay: () => void;
  /** Step by hours */
  step: (hours: number) => void;
  toggleGusts: () => void;
  toNow: () => void;
  close: () => void;
}

export function useMapKeys(keys: Keys) {
  const latest = useRef(keys);
  latest.current = keys;
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.ctrlKey || e.metaKey || e.altKey || e.defaultPrevented) return;
      const target = e.target instanceof HTMLElement ? e.target : null;
      if (target?.closest('input, textarea, select, [contenteditable="true"], .maplibregl-canvas')) return;
      const k = latest.current;
      const act = (f: () => void) => {
        e.preventDefault();
        f();
      };
      switch (e.key) {
        case ' ':
          if (!target?.closest('button, a')) act(k.togglePlay);
          break;
        case 'ArrowLeft':
          act(() => k.step(e.shiftKey ? -24 : -1));
          break;
        case 'ArrowRight':
          act(() => k.step(e.shiftKey ? 24 : 1));
          break;
        case 'g':
        case 'G':
          act(k.toggleGusts);
          break;
        case 'n':
        case 'N':
          act(k.toNow);
          break;
        case 'Escape':
          k.close();
          break;
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);
}
