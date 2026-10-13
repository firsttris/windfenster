/** The desktop map's timeline, as in the mockup: no card, it lies on the map. A thin track with
 * the days under it and "now" at its end; the hour is the knob's place (and the slider's value
 * for screen readers). Play and step stay on the keys (use-map-keys.ts). */

import { useState } from 'react';
import type { MapFrame } from '@/bindings/MapFrame';
import { intlLocale } from '@/lib/i18n';
import { clamp, daySegments, hourAt, onDay } from '@/lib/timeline';
import { cn } from '@/lib/utils';
import { m } from '@/paraglide/messages';
import { frameTime } from './timeline';

interface Props {
  frames: readonly MapFrame[];
  time: number;
  now: number;
  onTime: (t: number) => void;
  onPlay: (playing: boolean) => void;
  /** A CSS background for the track: the wind at the place picked, hour by hour */
  band?: string;
  className?: string;
}

/** Legible on any colour of the field, without a card under it */
const SHADOW = '[text-shadow:0_1px_3px_rgb(0_0_0/0.7)]';

export function TimelineSlim({ frames, time, now, onTime, onPlay, band, className }: Props) {
  const from = frames[0]?.validAt ?? 0;
  const to = frames.at(-1)?.validAt ?? 0;
  const span = Math.max(1, to - from);
  const at = (t: number) => ((clamp(t, from, to) - from) / span) * 100;
  const [hover, setHover] = useState<{ x: number; t: number } | null>(null);
  const dayFmt = new Intl.DateTimeFormat(intlLocale(), { weekday: 'short', day: 'numeric' });
  const go = (t: number) => {
    onPlay(false);
    onTime(clamp(t, from, to));
  };
  const nowInRange = now >= from && now <= to;
  const banded = band !== undefined && band !== 'transparent';

  return (
    <div className={cn('flex h-[52px] items-center gap-3 px-2 text-white', className)}>
      <div className="relative h-9 min-w-0 flex-1">
        {/* The track: the wind at the spot picked, or white with the hours gone by filled */}
        <div
          className="relative h-[18px]"
          onPointerMove={(e) => {
            if (e.pointerType !== 'mouse') return;
            const r = e.currentTarget.getBoundingClientRect();
            const x = e.clientX - r.left;
            setHover({ x, t: hourAt(x / r.width, from, to) });
          }}
          onPointerLeave={() => setHover(null)}
        >
          <span
            aria-hidden="true"
            className="absolute inset-x-0 top-[7px] h-1 rounded-full bg-white/35 shadow-[0_1px_2px_rgb(0_0_0/0.5)]"
            style={banded ? { background: band } : undefined}
          />
          {!banded && (
            <span
              aria-hidden="true"
              className="absolute top-[7px] left-0 h-1 rounded-full bg-white"
              style={{ width: `${at(time)}%` }}
            />
          )}
          {daySegments(from, to)
            .slice(1)
            .map((d) => (
              <span
                key={d.day}
                aria-hidden="true"
                className="absolute top-1 h-2.5 w-px bg-white/40"
                style={{ left: `${at(d.from)}%` }}
              />
            ))}
          {nowInRange && (
            <span
              aria-hidden="true"
              className="absolute top-0.5 h-3.5 w-px bg-white/80"
              style={{ left: `${at(now)}%` }}
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
            className="peer absolute inset-0 m-0 w-full cursor-pointer opacity-0"
          />
          <span
            aria-hidden="true"
            className="pointer-events-none absolute top-[2px] -ml-[7px] size-3.5 rounded-full bg-white shadow-[0_0_0_3px_rgb(9_9_11/0.6)] peer-focus-visible:ring-2 peer-focus-visible:ring-white/70"
            style={{ left: `${at(time)}%` }}
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
        {/* The days under it, a tap jumps to the same hour there */}
        <div className="relative h-[18px]">
          {daySegments(from, to).map((d) => {
            const active = time >= d.from && (time < d.to || d.to === to);
            return (
              <button
                key={d.day}
                type="button"
                onClick={() => go(onDay(time, d.day, from, to))}
                className={cn(
                  'absolute top-0 h-[18px] cursor-pointer truncate rounded border-0 bg-transparent px-[3px] text-left font-mono text-[10px] outline-none focus-visible:ring-2 focus-visible:ring-white/70',
                  SHADOW,
                  active ? 'font-medium text-white' : 'text-white/80 hover:text-white',
                )}
                style={{ left: `${at(d.from)}%`, width: `${at(d.to) - at(d.from)}%` }}
              >
                {dayFmt.format(new Date(d.day * 1000))}
              </button>
            );
          })}
        </div>
      </div>
      {nowInRange && (
        <button
          type="button"
          onClick={() => go(Math.round(now / 3600) * 3600)}
          className={cn(
            'h-8 shrink-0 cursor-pointer rounded-full border border-white/50 bg-transparent px-3 font-semibold text-white text-xs outline-none hover:bg-white/10 focus-visible:ring-2 focus-visible:ring-white/70',
            SHADOW,
          )}
        >
          {m.map_nowButton()}
        </button>
      )}
    </div>
  );
}
