import { LoaderCircle, Pause, Play, RotateCcw } from 'lucide-react';
import { useState } from 'react';
import type { MapFrame } from '@/bindings/MapFrame';
import { intlLocale } from '@/lib/i18n';
import { clamp, daySegments, hourAt, onDay } from '@/lib/timeline';
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
  /** A CSS background for the track: the wind at the place picked, hour by hour */
  band?: string;
  /** The hour shown is still loading */
  loading?: boolean;
  /** One row for phones: without the day segments */
  compact?: boolean;
  /** Without its own card, inside the spot sheet */
  bare?: boolean;
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

/** Play, the hours on a slider and the days as segments to jump to. The slider steps an hour
 * with the arrow keys and a day with Page up/down; "now" is marked and one tap away. */
export function Timeline({
  frames,
  time,
  now,
  playing,
  onTime,
  onPlay,
  band,
  loading = false,
  compact = false,
  bare = false,
  className,
}: Props) {
  const from = frames[0]?.validAt ?? 0;
  const to = frames.at(-1)?.validAt ?? 0;
  const span = Math.max(1, to - from);
  const pct = (t: number) => `${((clamp(t, from, to) - from) / span) * 100}%`;
  const hours = Math.round((time - now) / 3600);
  const [hover, setHover] = useState<{ x: number; t: number } | null>(null);
  const dayFmt = new Intl.DateTimeFormat(intlLocale(), { weekday: 'short', day: 'numeric' });
  const go = (t: number) => {
    onPlay(false);
    onTime(clamp(t, from, to));
  };
  const nowInRange = now >= from && now <= to;

  return (
    <div className={cn('flex items-center gap-3', !bare && 'glass rounded-xl border p-3', className)}>
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
          <span className="flex min-w-0 items-center gap-1.5">
            <span className="truncate font-semibold text-sm">{frameTime(time)}</span>
            {loading && (
              <LoaderCircle
                role="status"
                aria-label={m.map_loading()}
                className="size-3.5 shrink-0 animate-spin text-muted-foreground"
              />
            )}
          </span>
          {hours === 0 || !nowInRange ? (
            <span className="shrink-0 font-mono text-muted-foreground text-xs">
              {hours === 0 ? m.map_now() : m.map_hours({ hours: hours > 0 ? `+${hours}` : `−${-hours}` })}
            </span>
          ) : (
            <button
              type="button"
              onClick={() => go(Math.round(now / 3600) * 3600)}
              className="-my-1 inline-flex shrink-0 cursor-pointer items-center gap-1 rounded-md px-1.5 py-1 font-mono text-muted-foreground text-xs outline-none hover:bg-secondary hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/60"
              aria-label={m.map_backToNow()}
            >
              {m.map_hours({ hours: hours > 0 ? `+${hours}` : `−${-hours}` })}
              <RotateCcw aria-hidden="true" className="size-3" />
            </button>
          )}
        </div>
        {/* The track: the wind at the spot, "now", and the hour under the pointer */}
        <div
          className="relative"
          onPointerMove={(e) => {
            if (e.pointerType !== 'mouse') return;
            const r = e.currentTarget.getBoundingClientRect();
            const x = e.clientX - r.left;
            setHover({ x, t: hourAt(x / r.width, from, to) });
          }}
          onPointerLeave={() => setHover(null)}
        >
          {band && band !== 'transparent' && (
            <div
              aria-hidden="true"
              className="absolute inset-x-0 top-0 h-1.5 rounded-full"
              style={{ background: band }}
            />
          )}
          {nowInRange && (
            <div
              aria-hidden="true"
              className="pointer-events-none absolute top-0 h-5 w-px bg-foreground/50"
              style={{ left: pct(now) }}
            />
          )}
          <input
            type="range"
            aria-label={m.map_time()}
            aria-valuetext={frameTime(time)}
            min={from}
            max={to}
            step={3600}
            value={Math.round(time / 3600) * 3600}
            onChange={(e) => go(Number(e.target.value))}
            onKeyDown={(e) => {
              if (e.key !== 'PageUp' && e.key !== 'PageDown') return;
              e.preventDefault();
              go(time + (e.key === 'PageUp' ? 86_400 : -86_400));
            }}
            className={cn('relative w-full accent-foreground', band && band !== 'transparent' ? 'mt-2.5' : 'mt-1')}
          />
          {hover && (
            <span
              aria-hidden="true"
              className="pointer-events-none absolute bottom-full mb-1 -translate-x-1/2 whitespace-nowrap rounded-md border bg-popover px-1.5 py-0.5 font-mono text-[11px] text-popover-foreground shadow-sm"
              style={{ left: hover.x }}
            >
              {frameTime(hover.t)}
            </span>
          )}
        </div>
        {!compact && (
          <div className="relative h-5">
            {daySegments(from, to).map((d) => {
              const active = time >= d.from && (time < d.to || d.to === to);
              return (
                <button
                  key={d.day}
                  type="button"
                  onClick={() => go(onDay(time, d.day, from, to))}
                  className={cn(
                    'absolute top-0 h-5 cursor-pointer truncate rounded border-line border-l px-1 text-left font-mono text-[11px] outline-none focus-visible:ring-2 focus-visible:ring-ring/60',
                    active ? 'bg-secondary text-foreground' : 'text-muted-foreground hover:text-foreground',
                  )}
                  style={{ left: pct(d.from), width: `calc(${pct(d.to)} - ${pct(d.from)})` }}
                >
                  {dayFmt.format(new Date(d.day * 1000))}
                </button>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}
