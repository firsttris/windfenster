import type { Forecast } from '@/bindings/Forecast';
import type { GearItem } from '@/bindings/GearItem';
import type { Taf } from '@/bindings/Taf';
import type { WindUnit } from '@/bindings/WindUnit';
import type { Day } from '@/lib/forecast';
import type { Measured } from '@/lib/observations';
import { DayCard } from './day-card';

interface Props {
  fc: Forecast;
  days: Day[];
  unit: WindUnit;
  /** Unix seconds */
  now: number;
  measured?: Measured;
  taf?: Taf | null;
  gear?: readonly GearItem[];
}

/** The ten days, each a card: two side by side on wide screens, one under the other on phones. */
export function ForecastDays({ fc, days, unit, now, measured, taf, gear }: Props) {
  return (
    <div className="grid min-w-0 gap-3 lg:grid-cols-2">
      {days.map((d) => (
        <DayCard key={d.date} fc={fc} day={d} unit={unit} now={now} measured={measured} taf={taf} gear={gear} />
      ))}
    </div>
  );
}
