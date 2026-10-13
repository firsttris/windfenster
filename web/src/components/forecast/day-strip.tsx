/** The ten days in one strip across a phone, as Windfinder has it: per day its wind every 3 hours
 * as small bars in the scale's colour; today is marked, and a tap jumps to the day's card. */
import type { Day } from '@/lib/forecast';
import { weekday } from '@/lib/format';
import { cn } from '@/lib/utils';
import { m } from '@/paraglide/messages';
import { windBand } from '@/ui/wind-scale';

/** The bars' full height in pixels, reached at 30 kn */
const BAR = 30;

export function DayStrip({ days, today, className }: { days: Day[]; today: string; className?: string }) {
  const go = (date: string) => document.getElementById(`day-${date}`)?.scrollIntoView({ behavior: 'smooth' });
  return (
    <nav aria-label={m.strip_label()} className={cn('flex gap-px rounded-xl border bg-card p-[3px]', className)}>
      {days.map((d) => (
        <button
          key={d.date}
          type="button"
          onClick={() => go(d.date)}
          aria-current={d.date === today ? 'date' : undefined}
          className={cn(
            'flex min-w-0 flex-1 cursor-pointer flex-col items-stretch gap-[3px] rounded-md border-0 bg-transparent px-0.5 pt-1 pb-[3px] outline-none focus-visible:ring-2 focus-visible:ring-ring/60',
            d.date === today && 'bg-secondary',
          )}
        >
          <span className="text-center font-semibold text-[10px] leading-[1.1]">
            {weekday(d.date, 'short').replace(/\.$/, '')}
            <br />
            <span className="font-mono font-normal text-muted-foreground">{Number(d.date.slice(8))}</span>
          </span>
          <span className="flex h-[30px] items-end" aria-hidden="true">
            {d.cells.map((c) => (
              <span
                key={c.time}
                className="flex-1"
                style={{
                  height: Math.max(2, Math.min(1, (c.wind ?? 0) / 30) * BAR),
                  background: c.wind === null ? 'transparent' : windBand(c.wind).bg,
                }}
              />
            ))}
          </span>
        </button>
      ))}
    </nav>
  );
}
