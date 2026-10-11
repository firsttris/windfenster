import { useSyncExternalStore } from 'react';

/** Whether a media query matches, kept current. */
export function useMedia(query: string, serverDefault = false): boolean {
  return useSyncExternalStore(
    (onChange) => {
      const m = matchMedia(query);
      m.addEventListener('change', onChange);
      return () => m.removeEventListener('change', onChange);
    },
    () => matchMedia(query).matches,
    () => serverDefault,
  );
}
