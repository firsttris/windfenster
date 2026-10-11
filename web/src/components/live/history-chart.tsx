import 'uplot/dist/uPlot.min.css';
import { useEffect, useRef, useState } from 'react';
import uPlot from 'uplot';
import type { Observation } from '@/bindings/Observation';
import { localClock } from '@/lib/forecast';
import { clock, compass, dayMonthOf } from '@/lib/format';
import { arrowPicks, BEAUFORT_KN, type DayOption, isolated, TIP, tipPlacement } from '@/lib/history';
import { intlLocale } from '@/lib/i18n';
import { useTheme, useUnit } from '@/lib/prefs';
import { formatWind, KMH_PER_KN, MS_PER_KN, UNIT_LABEL } from '@/lib/units';
import { cn } from '@/lib/utils';
import { m } from '@/paraglide/messages';
import { WindArrow } from '@/ui/wind-arrow';
import { WIND_SCALE } from '@/ui/wind-scale';

const HEIGHT = 240;
const FONT = '11px Geist Mono Variable, monospace';

/** A CSS colour as the canvas needs it: var(--x) resolved against the page. */
function resolve(color: string): string {
  const v = color.match(/^var\((--[\w-]+)\)$/);
  return v ? getComputedStyle(document.documentElement).getPropertyValue(v[1]).trim() : color;
}

/** `color` (hex) at `alpha`, for the canvas */
function fade(color: string, alpha: number): string {
  const n = Number.parseInt(color.replace('#', ''), 16);
  return `rgba(${(n >> 16) & 255},${(n >> 8) & 255},${n & 255},${alpha})`;
}

/** The arrow of WindArrow, centred on (x, y), `size` px, pointing where the wind blows. */
function drawArrow(ctx: CanvasRenderingContext2D, x: number, y: number, deg: number, size: number) {
  ctx.save();
  ctx.translate(x, y);
  ctx.rotate(((deg + 180) * Math.PI) / 180);
  const s = size / 24;
  ctx.beginPath();
  ctx.moveTo(0, -9 * s);
  ctx.lineTo(6.5 * s, 8 * s);
  ctx.lineTo(0, 4 * s);
  ctx.lineTo(-6.5 * s, 8 * s);
  ctx.closePath();
  ctx.fill();
  ctx.restore();
}

/** The point under the pointer: which measurement, and where on the chart (CSS pixels). */
interface Hover {
  i: number;
  x: number;
  y: number;
}

/** A day's measurements: mean wind as a blue line over a soft blue area, gusts in red, the
 * direction as arrows above, the wind scale faintly behind. Pointer, finger or arrow keys show
 * the values at that moment right there. */
export function HistoryChart({ obs, day, timeZone }: { obs: Observation[]; day: DayOption; timeZone: string }) {
  const [unit] = useUnit();
  const [theme] = useTheme();
  const wrap = useRef<HTMLDivElement>(null);
  const box = useRef<HTMLDivElement>(null);
  const plotRef = useRef<uPlot | null>(null);
  const [hover, setHover] = useState<Hover | null>(null);
  const factor = unit === 'kn' ? 1 : unit === 'ms' ? MS_PER_KN : KMH_PER_KN;

  // biome-ignore lint/correctness/useExhaustiveDependencies: the colours come from the page, so a theme change redraws
  useEffect(() => {
    const el = box.current;
    if (!el) return;
    const fg = resolve('var(--muted-foreground)');
    const text = resolve('var(--foreground)');
    const grid = resolve('var(--line)');
    const card = resolve('var(--card)');
    const windColor = resolve('var(--chart-wind)');
    const gustColor = resolve('var(--chart-gust)');
    const dark = document.documentElement.classList.contains('dark');
    const lc = localClock(timeZone);
    const weekday = new Intl.DateTimeFormat(intlLocale(), { weekday: 'short', timeZone });
    const midnights: number[] = [];
    const ticks: number[] = [];
    for (let t = day.from - (day.from % 3600); t <= day.to; t += 3600) {
      if (t < day.from) continue;
      const h = lc(t).hour;
      if (h === 0) midnights.push(t);
      if (h % 3 === 0) ticks.push(t);
    }
    const x = obs.map((o) => o.at);
    const mean = obs.map((o) => (o.wind === null ? null : o.wind * factor));
    const gust = obs.map((o) => (o.gust === null ? null : o.gust * factor));
    const top = Math.max(...obs.map((o) => Math.max(o.wind ?? 0, o.gust ?? 0))) * factor;
    const step = unit === 'kmh' ? 10 : 5;
    const ymax = Math.max(step * 2, Math.ceil((top + step / 2) / step) * step);

    const plugin: uPlot.Plugin = {
      hooks: {
        drawClear: [
          (u) => {
            // The wind scale's bands, faint enough to leave the lines in front
            const { left, top: t, width, height } = u.bbox;
            const ctx = u.ctx;
            ctx.save();
            ctx.globalAlpha = dark ? 0.08 : 0.06;
            WIND_SCALE.forEach((b, i) => {
              const next = WIND_SCALE[i + 1]?.from ?? 1_000;
              const y0 = Math.min(t + height, u.valToPos(b.from * factor, 'y', true));
              const y1 = Math.max(t, u.valToPos(next * factor, 'y', true));
              if (y0 <= y1) return;
              ctx.fillStyle = b.bg;
              ctx.fillRect(left, y1, width, y0 - y1);
            });
            ctx.restore();
          },
        ],
        draw: [
          (u) => {
            const ctx = u.ctx;
            const { left, top: t, width, height } = u.bbox;
            const px = devicePixelRatio;
            ctx.save();
            // Midnight: a line, as Windfinder marks the day change
            ctx.strokeStyle = text;
            ctx.globalAlpha = 0.35;
            ctx.lineWidth = px;
            for (const mid of midnights) {
              const mx = u.valToPos(mid, 'x', true);
              if (mx <= left || mx >= left + width) continue;
              ctx.beginPath();
              ctx.moveTo(mx, t);
              ctx.lineTo(mx, t + height);
              ctx.stroke();
            }
            ctx.globalAlpha = 1;
            ctx.fillStyle = text;
            for (const o of arrowPicks(obs, day.from, day.to, width / px)) {
              drawArrow(ctx, u.valToPos(o.at, 'x', true), t - 13 * px, o.dir ?? 0, 18 * px);
            }
            ctx.restore();
          },
        ],
        setCursor: [
          (u) => {
            const i = u.cursor.idx;
            const o = i == null ? undefined : obs[i];
            if (i == null || !o || u.cursor.left == null || u.cursor.left < 0) {
              setHover(null);
              return;
            }
            // Above the higher of the two points
            const kn = Math.max(o.wind ?? 0, o.gust ?? 0) * factor;
            setHover({
              i,
              x: u.over.offsetLeft + u.valToPos(o.at, 'x'),
              y: u.over.offsetTop + u.valToPos(kn, 'y'),
            });
          },
        ],
        ready: [
          (u) => {
            // Fingers: sliding sideways moves the pointer, up and down still scrolls the page.
            u.over.style.touchAction = 'pan-y';
            const follow = (e: TouchEvent) => {
              const touch = e.touches[0];
              if (!touch) return;
              const r = u.over.getBoundingClientRect();
              u.setCursor({ left: touch.clientX - r.left, top: touch.clientY - r.top });
            };
            u.over.addEventListener('touchstart', follow, { passive: true });
            u.over.addEventListener('touchmove', follow, { passive: true });
          },
        ],
      },
    };
    const bftSplits = BEAUFORT_KN.map((kn) => kn * factor).filter((v) => v <= ymax);
    const opts: uPlot.Options = {
      width: el.clientWidth,
      height: HEIGHT,
      padding: [30, 4, 0, 0],
      legend: { show: false },
      cursor: {
        drag: { x: false, y: false },
        y: false,
        points: {
          size: 10,
          width: 2,
          stroke: () => card,
          fill: (_u, s) => (s === 1 ? windColor : gustColor),
        },
      },
      scales: { x: { time: false, range: [day.from, day.to] }, y: { range: [0, ymax] } },
      axes: [
        {
          stroke: fg,
          grid: { show: false },
          ticks: { show: true, stroke: grid, size: 4 },
          font: FONT,
          // Every three hours, on a narrow chart every six
          splits: (u) => {
            const every = u.bbox.width / devicePixelRatio < 520 ? 6 : 3;
            return ticks.filter((t) => lc(t).hour % every === 0);
          },
          values: (_u, splits) =>
            splits.map((t) => (lc(t).hour === 0 ? weekday.format(new Date(t * 1000)) : clock(t, timeZone))),
        },
        {
          stroke: fg,
          grid: { stroke: grid, width: 1, dash: [3, 4] },
          ticks: { show: false },
          font: FONT,
          size: 34,
          incrs: [step, step * 2, step * 4],
          space: 22,
        },
        {
          side: 1,
          scale: 'y',
          stroke: fg,
          grid: { show: false },
          ticks: { show: false },
          font: FONT,
          size: 30,
          label: 'bft',
          labelSize: 14,
          labelFont: FONT,
          splits: () => bftSplits,
          values: (_u, splits) => splits.map((_, i) => String(i + 1)),
        },
      ],
      series: [
        {},
        {
          label: m.history_mean(),
          stroke: windColor,
          width: 2.25,
          points: { show: false },
          // A soft area under the wind, fading towards calm
          fill: (u) => {
            const g = u.ctx.createLinearGradient(0, u.bbox.top, 0, u.bbox.top + u.bbox.height);
            g.addColorStop(0, fade(windColor, dark ? 0.32 : 0.24));
            g.addColorStop(1, fade(windColor, 0.02));
            return g;
          },
        },
        {
          label: m.history_gusts(),
          stroke: gustColor,
          width: 1.5,
          // A gust between two reports without one is a dot.
          points: { show: true, size: 5, width: 0, fill: gustColor, filter: (u, si) => isolated(u.data[si]) },
        },
      ],
      plugins: [plugin],
    };
    const plot = new uPlot(opts, [x, mean, gust] as uPlot.AlignedData, el);
    plotRef.current = plot;
    const resize = new ResizeObserver(() => plot.setSize({ width: el.clientWidth, height: HEIGHT }));
    resize.observe(el);
    // A tap elsewhere puts the card away (a finger never leaves the chart the way a mouse does).
    const away = (e: PointerEvent) => {
      if (!wrap.current?.contains(e.target as Node)) plot.setCursor({ left: -10, top: -10 });
    };
    document.addEventListener('pointerdown', away);
    return () => {
      document.removeEventListener('pointerdown', away);
      resize.disconnect();
      plot.destroy();
      plotRef.current = null;
      setHover(null);
    };
  }, [obs, day, timeZone, factor, unit, theme]);

  /** Arrow keys step through the measurements. */
  const onKey = (e: React.KeyboardEvent) => {
    const u = plotRef.current;
    if (!u || (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight' && e.key !== 'Escape')) return;
    e.preventDefault();
    if (e.key === 'Escape') {
      u.setCursor({ left: -10, top: -10 });
      return;
    }
    const at = hover ? hover.i + (e.key === 'ArrowRight' ? 1 : -1) : e.key === 'ArrowRight' ? 0 : obs.length - 1;
    const i = Math.min(obs.length - 1, Math.max(0, at));
    u.setCursor({ left: u.valToPos(obs[i].at, 'x'), top: 10 });
  };

  const o = hover ? obs[hover.i] : null;
  const dayLabel = day.key === 'today' ? m.history_last24() : dayMonthOf(day.from + 12 * 3600, timeZone);
  const value = (kn: number | null) => (kn === null ? '–' : formatWind(kn, unit));
  const width = box.current?.clientWidth ?? 0;
  const place = hover ? tipPlacement(hover.x, hover.y, width) : null;
  const spoken = o
    ? [
        clock(o.at, timeZone),
        `${m.history_mean()} ${value(o.wind)} ${UNIT_LABEL[unit]}`,
        o.gust !== null ? `${m.history_gusts()} ${value(o.gust)} ${UNIT_LABEL[unit]}` : '',
        o.dir !== null ? `${compass(o.dir)} ${Math.round(o.dir)}°` : '',
      ]
        .filter(Boolean)
        .join(', ')
    : '';
  return (
    <div className="flex flex-col gap-2">
      <div className="flex min-h-5 flex-wrap items-center gap-x-4 gap-y-1 text-xs">
        <Legend color="var(--chart-wind)" label={m.history_mean()} thick />
        <Legend color="var(--chart-gust)" label={m.history_gusts()} />
        <span className="ml-auto hidden text-muted-foreground sm:inline">{m.history_hint()}</span>
      </div>
      {/* A slider over the measurement times: arrow keys step through them, the value is read out */}
      <div
        ref={wrap}
        role="slider"
        tabIndex={0}
        onKeyDown={onKey}
        aria-label={m.history_aria({ day: dayLabel })}
        aria-valuemin={0}
        aria-valuemax={Math.max(0, obs.length - 1)}
        aria-valuenow={hover?.i ?? 0}
        aria-valuetext={spoken || m.history_hint()}
        className="wf-chart relative rounded-lg outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
      >
        <div ref={box} aria-hidden="true" className="w-full" />
        {o && place && (
          <div
            aria-hidden="true"
            className="pointer-events-none absolute z-10 flex flex-col gap-1 rounded-lg border bg-popover/95 px-3 py-2 text-xs shadow-lg backdrop-blur"
            style={{ left: place.left, top: place.top, width: TIP.width }}
          >
            <span className="font-mono text-muted-foreground">{clock(o.at, timeZone)}</span>
            <TipRow color="var(--chart-wind)" label={m.history_mean()} value={value(o.wind)} unit={UNIT_LABEL[unit]} />
            {o.gust !== null && (
              <TipRow
                color="var(--chart-gust)"
                label={m.history_gusts()}
                value={value(o.gust)}
                unit={UNIT_LABEL[unit]}
              />
            )}
            {o.dir !== null && (
              <span className="flex items-center gap-1.5 text-muted-foreground">
                <WindArrow deg={o.dir} className="size-3.5 text-foreground" />
                {compass(o.dir)} · {Math.round(o.dir)}°
              </span>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

function TipRow({ color, label, value, unit }: { color: string; label: string; value: string; unit: string }) {
  return (
    <span className="flex items-baseline justify-between gap-2">
      <span className="flex items-center gap-1.5 text-muted-foreground">
        <span className="size-2 rounded-full" style={{ background: color }} />
        {label}
      </span>
      <span>
        <span className="font-mono font-semibold text-foreground text-sm">{value}</span>{' '}
        <span className="text-muted-foreground">{unit}</span>
      </span>
    </span>
  );
}

function Legend({ color, label, thick = false }: { color: string; label: string; thick?: boolean }) {
  return (
    <span className="flex items-center gap-1.5 text-muted-foreground">
      <span className={cn('w-3.5 rounded-full', thick ? 'h-[3px]' : 'h-0.5')} style={{ background: color }} />
      {label}
    </span>
  );
}
