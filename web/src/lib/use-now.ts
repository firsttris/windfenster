import { useEffect, useState } from 'react';

/** Unix seconds, refreshed every `everyMs`, so past hours fade without a reload. */
export function useNow(everyMs = 60_000): number {
  const [now, setNow] = useState(() => Math.floor(Date.now() / 1000));
  useEffect(() => {
    const t = setInterval(() => setNow(Math.floor(Date.now() / 1000)), everyMs);
    return () => clearInterval(t);
  }, [everyMs]);
  return now;
}
