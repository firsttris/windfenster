/** A station's last hours as a sparkline: the wind as a line with a soft area under it, gusts as
 * dots, on a scale from 0 to the strongest value (at least 10 kn, so calm stays low). */
import type { Observation } from '@/bindings/Observation';

export interface Spark {
  line: string;
  area: string;
  gusts: { x: number; y: number }[];
  max: number;
}

export function spark(
  obs: readonly Observation[],
  from: number,
  to: number,
  width: number,
  height: number,
): Spark | null {
  const winds = obs.filter((o) => o.wind !== null && o.at >= from && o.at <= to);
  if (winds.length < 2) return null;
  const max = Math.max(10, ...obs.flatMap((o) => [o.wind ?? 0, o.gust ?? 0]));
  const x = (t: number) => ((t - from) / Math.max(1, to - from)) * width;
  const y = (kn: number) => height - (kn / max) * (height - 2) - 1;
  const pts = winds.map((o) => `${x(o.at).toFixed(1)},${y(o.wind as number).toFixed(1)}`);
  const line = `M${pts.join('L')}`;
  const first = x(winds[0].at).toFixed(1);
  const last = x(winds[winds.length - 1].at).toFixed(1);
  return {
    line,
    area: `${line}L${last},${height}L${first},${height}Z`,
    gusts: obs
      .filter((o) => o.gust !== null && o.at >= from && o.at <= to)
      .map((o) => ({ x: x(o.at), y: y(o.gust as number) })),
    max,
  };
}
