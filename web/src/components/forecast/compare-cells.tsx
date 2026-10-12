/** The cells that check the forecast: what was measured and what the TAF says, per hour. */
import type { Taf } from '@/bindings/Taf';
import type { WindUnit } from '@/bindings/WindUnit';
import type { Cell } from '@/lib/forecast';
import { type Measured, nearest } from '@/lib/observations';
import { tafHour } from '@/lib/taf-text';
import { formatWind } from '@/lib/units';
import { cn } from '@/lib/utils';
import { windBand } from '@/ui/wind-scale';

/** The measurement at a past hour, framed in its scale colour; a quiet line for hours to come. */
export function MeasuredCell({
  cell,
  now,
  measured,
  unit,
}: {
  cell: Cell;
  now: number;
  measured: Measured;
  unit: WindUnit;
}) {
  const o = nearest(measured.observations, cell.time);
  if (o?.wind == null) {
    // Hours to come, and the last ones while their values are on the way (DWD runs about an hour
    // behind), get the quiet line; a dash marks a gap in the measurements.
    const last = measured.observations.at(-1)?.at ?? 0;
    return cell.time > now || cell.time > last ? (
      <span className="mx-auto block h-0.5 w-11 rounded-full bg-line" />
    ) : (
      <Dash />
    );
  }
  // A gust the report has goes beside the wind: wind with wind, gust with gust
  return (
    <span
      className="mx-auto flex h-7 min-w-11 items-baseline justify-center gap-px whitespace-nowrap rounded-[7px] border-2 px-1 pt-[3px] font-bold font-mono text-sm"
      style={{ borderColor: windBand(o.wind).bg }}
    >
      {formatWind(o.wind, unit)}
      {o.gust != null && o.gust > o.wind && (
        <span className="font-medium text-[10px] text-muted-foreground">/{formatWind(o.gust, unit)}</span>
      )}
    </span>
  );
}

/** The TAF for an hour: its prevailing wind, or the span TEMPO allows, in a dashed frame. */
export function TafCell({ cell, taf, unit }: { cell: Cell; taf: Taf; unit: WindUnit }) {
  const h = tafHour(taf, cell.time);
  if (h?.wind == null) return <span className="mx-auto block h-0.5 w-11 rounded-full bg-line" />;
  const range = h.low !== null && h.high !== null;
  return (
    <span
      className={cn(
        'mx-auto flex h-6 min-w-11 items-center justify-center whitespace-nowrap rounded-[6px] border border-dashed px-1 font-mono font-semibold',
        range ? 'text-[11px]' : 'text-[13px]',
      )}
      style={{ borderColor: windBand(h.high ?? h.wind).bg }}
      title={h.probability ? `PROB${h.probability}` : undefined}
    >
      {range ? `${formatWind(h.low ?? 0, unit)}–${formatWind(h.high ?? 0, unit)}` : formatWind(h.wind, unit)}
    </span>
  );
}

export function Dash() {
  return <span className="text-dim">–</span>;
}
