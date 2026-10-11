import { useEffect, useRef } from 'react';
import type { WindUnit } from '@/bindings/WindUnit';
import { compass } from '@/lib/format';
import { intlLocale } from '@/lib/i18n';
import type { SpotHour } from '@/lib/spot-series';
import { formatWind, UNIT_LABEL } from '@/lib/units';
import { cn } from '@/lib/utils';
import { m } from '@/paraglide/messages';
import { WindArrow } from '@/ui/wind-arrow';
import { windBand } from '@/ui/wind-scale';

interface Props {
  hours: readonly SpotHour[];
  /** The hour the map shows */
  time: number | null;
  unit: WindUnit;
  onTime: (t: number) => void;
  className?: string;
}

const hourFmt = () => new Intl.DateTimeFormat(intlLocale(), { hour: '2-digit', hourCycle: 'h23' });
const dayFmt = () => new Intl.DateTimeFormat(intlLocale(), { weekday: 'short', day: 'numeric' });
const fullFmt = () =>
  new Intl.DateTimeFormat(intlLocale(), {
    weekday: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
    hourCycle: 'h23',
  });

/** The place's hours side by side, as in the forecast table: time, direction, wind in the
 * scale's colour, gusts under it. The hour on the map is marked; tapping one goes there. */
export function SpotStrip({ hours, time, unit, onTime, className }: Props) {
  const scroller = useRef<HTMLFieldSetElement>(null);
  const shown = time === null ? -1 : nearest(hours, time);
  const hour = hourFmt();
  const day = dayFmt();
  const full = fullFmt();

  // Keep the hour on the map in view, scrolling only the strip (not the page).
  useEffect(() => {
    const box = scroller.current;
    const el = shown >= 0 ? box?.querySelectorAll<HTMLElement>('button')[shown] : undefined;
    if (!box || !el) return;
    const left = el.offsetLeft - (box.clientWidth - el.offsetWidth) / 2;
    box.scrollTo({ left, behavior: 'smooth' });
  }, [shown]);

  return (
    <fieldset
      ref={scroller}
      aria-label={m.map_spotHours()}
      className={cn(
        'relative m-0 -mx-1 flex min-w-0 overflow-x-auto border-0 px-1 pt-0 pb-1 [scrollbar-width:thin]',
        className,
      )}
    >
      {/* What the rows are, staying in place while the hours scroll */}
      <div
        aria-hidden="true"
        className="sticky left-0 z-10 flex shrink-0 flex-col items-end gap-1 bg-card pt-4 pr-1.5 pb-1 font-mono text-[10px] text-muted-foreground"
      >
        <span className="leading-[15px]">&nbsp;</span>
        <span className="h-3.5" />
        <span className="py-0.5 text-xs leading-4">{UNIT_LABEL[unit]}</span>
        <span className="leading-[15px]">{m.map_layerGusts()}</span>
      </div>
      {hours.map((h, i) => {
        const d = new Date(h.validAt * 1000);
        const newDay = i === 0 || new Date(hours[i - 1].validAt * 1000).getDate() !== d.getDate();
        const band = windBand(h.wind);
        const label = [
          full.format(d),
          `${formatWind(h.wind, unit)} ${UNIT_LABEL[unit]}`,
          h.gust !== null ? m.map_gust({ gust: formatWind(h.gust, unit), unit: UNIT_LABEL[unit] }) : null,
          compass(h.dir),
        ]
          .filter(Boolean)
          .join(', ');
        return (
          <button
            key={h.validAt}
            type="button"
            aria-label={label}
            aria-current={i === shown}
            onClick={() => onTime(h.validAt)}
            className={cn(
              'relative flex w-9 shrink-0 cursor-pointer flex-col items-center gap-1 rounded-md pt-4 pb-1 outline-none focus-visible:ring-2 focus-visible:ring-ring/60',
              newDay && i > 0 && 'ml-1.5 border-line border-l pl-0.5',
              i === shown ? 'bg-secondary' : 'hover:bg-secondary/60',
            )}
          >
            {newDay && (
              <span className="absolute top-0 left-1 whitespace-nowrap font-medium text-[10px] text-foreground">
                {day.format(d)}
              </span>
            )}
            <span className="font-mono text-[10px] text-muted-foreground">{hour.format(d)}</span>
            <WindArrow deg={h.dir} className="size-3.5 text-foreground" />
            <span
              className="w-8 rounded py-0.5 text-center font-mono font-semibold text-xs"
              style={{ background: band.bg, color: band.fg }}
            >
              {formatWind(h.wind, unit)}
            </span>
            <span className="font-mono text-[10px] text-muted-foreground">
              {h.gust !== null ? formatWind(h.gust, unit) : '–'}
            </span>
          </button>
        );
      })}
    </fieldset>
  );
}

/** The hour closest to `time` */
function nearest(hours: readonly SpotHour[], time: number): number {
  let best = -1;
  for (let i = 0; i < hours.length; i++) {
    if (best < 0 || Math.abs(hours[i].validAt - time) < Math.abs(hours[best].validAt - time)) best = i;
  }
  return best;
}
