import type { WindUnit } from '@/bindings/WindUnit';
import { KMH_PER_KN, MS_PER_KN, UNIT_LABEL } from '@/lib/units';
import { cn } from '@/lib/utils';
import { RAMP_MAX_KN, rampGradient } from '@/lib/wind-map';
import { m } from '@/paraglide/messages';

/** Round marks per unit within the ramp's 0…40 kn */
const TICKS: Record<WindUnit, number[]> = {
  kn: [0, 10, 20, 30, 40],
  ms: [0, 5, 10, 15, 20],
  kmh: [0, 20, 40, 60],
};

const TO_KN: Record<WindUnit, number> = { kn: 1, ms: 1 / MS_PER_KN, kmh: 1 / KMH_PER_KN };

/** The colour field's scale in the chosen unit; wind and gusts share it. */
export function Legend({ unit, gusts, className }: { unit: WindUnit; gusts: boolean; className?: string }) {
  const label = gusts ? m.map_legendGusts : m.map_legend;
  return (
    <figure className={cn('m-0 flex flex-col gap-1.5', className)}>
      <div
        role="img"
        aria-label={label({ unit: UNIT_LABEL[unit] })}
        className="h-2 rounded-full"
        style={{ background: rampGradient() }}
      />
      <figcaption className="relative h-4 font-mono text-[11px] text-muted-foreground" aria-hidden="true">
        {TICKS[unit].map((t, i, all) => {
          // Marks near the right end stand left of their place, so the unit fits.
          const at = (t * TO_KN[unit]) / RAMP_MAX_KN;
          return (
            <span
              key={t}
              className={cn(
                'absolute whitespace-nowrap',
                i === 0 ? '' : at > 0.9 ? '-translate-x-full' : '-translate-x-1/2',
              )}
              style={{ left: `${at * 100}%` }}
            >
              {t}
              {i === all.length - 1 && ` ${UNIT_LABEL[unit]}`}
            </span>
          );
        })}
      </figcaption>
    </figure>
  );
}
