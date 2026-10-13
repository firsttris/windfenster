/** The TAF along a day's hours as sections: hours in a row that show the same value (the
 * prevailing wind, or the span TEMPO and PROB allow) become one section, so the value is written
 * once and a line shows how long it holds. Hours that are over carry none: what was measured is
 * shown elsewhere. Pure, for the table and the phone's rows alike. */
import type { Taf } from '@/bindings/Taf';
import type { WindUnit } from '@/bindings/WindUnit';
import { tafHour } from './taf-text';
import { formatWind } from './units';

export interface TafSegment {
  /** Index of the first hour */
  from: number;
  /** Number of hours */
  span: number;
  /** "12" or "8–14" in the unit; null for hours without a TAF value */
  text: string | null;
  /** The strongest wind of the section in kn, for its colour */
  high: number;
}

export function tafSegments(
  cells: readonly { time: number; past: boolean }[],
  taf: Taf | null | undefined,
  unit: WindUnit,
): TafSegment[] {
  const out: TafSegment[] = [];
  cells.forEach((c, i) => {
    const h = taf && !c.past ? tafHour(taf, c.time) : null;
    const range = h?.low != null && h?.high != null;
    const text =
      h?.wind == null
        ? null
        : range
          ? `${formatWind(h.low ?? 0, unit)}–${formatWind(h.high ?? 0, unit)}`
          : formatWind(h.wind, unit);
    const high = h ? (h.high ?? h.wind ?? 0) : 0;
    const last = out.at(-1);
    if (last && last.text === text && last.from + last.span === i) {
      last.span += 1;
      last.high = Math.max(last.high, high);
    } else {
      out.push({ from: i, span: 1, text, high });
    }
  });
  return out;
}
