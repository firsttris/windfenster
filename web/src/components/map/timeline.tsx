import { Pause, Play } from 'lucide-react';
import type { MapFrame } from '@/bindings/MapFrame';
import { intlLocale } from '@/lib/i18n';
import { cn } from '@/lib/utils';
import { m } from '@/paraglide/messages';
import { Button } from '@/ui/button';

interface Props {
  frames: readonly MapFrame[];
  time: number;
  now: number;
  playing: boolean;
  onTime: (t: number) => void;
  onPlay: (playing: boolean) => void;
  className?: string;
}

/** "So., 11. Okt., 14:00" in the viewer's time zone */
export function frameTime(unix: number): string {
  return new Intl.DateTimeFormat(intlLocale(), {
    weekday: 'short',
    day: 'numeric',
    month: 'short',
    hour: '2-digit',
    minute: '2-digit',
    hourCycle: 'h23',
  }).format(new Date(unix * 1000));
}

/** Local midnights within the span, for the day marks under the slider */
function days(from: number, to: number): { at: number; label: string }[] {
  const fmt = new Intl.DateTimeFormat(intlLocale(), { weekday: 'short', day: 'numeric' });
  const d = new Date(from * 1000);
  d.setHours(24, 0, 0, 0);
  const out: { at: number; label: string }[] = [];
  while (d.getTime() / 1000 < to) {
    out.push({ at: d.getTime() / 1000, label: fmt.format(d) });
    d.setDate(d.getDate() + 1);
  }
  return out;
}

/** Play and the hours of the model on a slider, with the days marked. */
export function Timeline({ frames, time, now, playing, onTime, onPlay, className }: Props) {
  const from = frames[0]?.validAt ?? 0;
  const to = frames.at(-1)?.validAt ?? 0;
  const span = Math.max(1, to - from);
  const hours = Math.round((time - now) / 3600);
  return (
    <div className={cn('flex items-center gap-3 rounded-xl border bg-card/95 p-3 backdrop-blur', className)}>
      <Button
        size="icon"
        aria-label={playing ? m.map_pause() : m.map_play()}
        onClick={() => onPlay(!playing)}
        className="shrink-0"
      >
        {playing ? <Pause aria-hidden="true" /> : <Play aria-hidden="true" />}
      </Button>
      <div className="flex min-w-0 flex-1 flex-col gap-1.5">
        <div className="flex items-baseline justify-between gap-3">
          <span className="truncate font-semibold text-sm">{frameTime(time)}</span>
          <span className="shrink-0 font-mono text-muted-foreground text-xs">
            {hours === 0 ? m.map_now() : m.map_hours({ hours: hours > 0 ? `+${hours}` : `−${-hours}` })}
          </span>
        </div>
        <input
          type="range"
          aria-label={m.map_time()}
          aria-valuetext={frameTime(time)}
          min={from}
          max={to}
          step={3600}
          value={Math.round(time / 3600) * 3600}
          onChange={(e) => {
            onPlay(false);
            onTime(Number(e.target.value));
          }}
          className="w-full accent-foreground"
        />
        <div className="relative h-4 font-mono text-[11px] text-muted-foreground" aria-hidden="true">
          {days(from, to).map((d) => (
            <span
              key={d.at}
              className="absolute -translate-x-1/2 whitespace-nowrap"
              style={{ left: `${((d.at - from) / span) * 100}%` }}
            >
              {d.label}
            </span>
          ))}
        </div>
      </div>
    </div>
  );
}
