import 'uplot/dist/uPlot.min.css';
import { useQuery } from '@tanstack/react-query';
import { useEffect, useMemo, useRef, useState } from 'react';
import uPlot from 'uplot';
import type { Observation } from '@/bindings/Observation';
import type { Station } from '@/bindings/Station';
import { historyQuery } from '@/lib/api';
import { localClock } from '@/lib/forecast';
import { clock, compass, dayMonthOf } from '@/lib/format';
import { arrowPicks, BEAUFORT_KN, type DayOption, dayOptions, dayStats } from '@/lib/history';
import { intlLocale } from '@/lib/i18n';
import { useTheme, useUnit } from '@/lib/prefs';
import { formatWind, KMH_PER_KN, MS_PER_KN, UNIT_LABEL } from '@/lib/units';
import { useNow } from '@/lib/use-now';
import { cn } from '@/lib/utils';
import { m } from '@/paraglide/messages';
import { Card } from '@/ui/card';
import { WindArrow } from '@/ui/wind-arrow';
import { WIND_SCALE } from '@/ui/wind-scale';

const HEIGHT = 240;

/** A CSS colour as the canvas needs it: var(--x) resolved against the page. */
function resolve(color: string): string {
  const v = color.match(/^var\((--[\w-]+)\)$/);
  return v ? getComputedStyle(document.documentElement).getPropertyValue(v[1]).trim() : color;
}

/** The measurements of a day as Windfinder shows them: mean wind and gusts as lines, the
 * direction as arrows above, bands by wind strength behind; today (the last 24 hours) and
 * the ten days before. */
export function HistoryCard({ station, timeZone }: { station: Station; timeZone: string }) {
  const [unit] = useUnit();
  const now = useNow();
  // The 24-hour window moves in steps of five minutes, so a minute does not start a new query.
  const until = now - (now % 300);
  const days = useMemo(() => dayOptions(until, timeZone), [until, timeZone]);
  const [key, setKey] = useState('today');
  const day = days.find((d) => d.key === key) ?? days[0];
  const q = useQuery(historyQuery(station.slug, day.from, day.to, day.key === 'today'));
  const obs = q.data?.observations ?? [];
  const stats = dayStats(obs);
  const source = `${station.source === 'metar' ? 'METAR' : 'DWD'} ${station.code}`;
  const interval = station.source === 'metar' ? m.history_metar() : m.history_dwd();
  const speed = (kn: number | null) => (kn === null ? '–' : `${formatWind(kn, unit)} ${UNIT_LABEL[unit]}`);

  return (
    <Card aria-labelledby="history-title">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h2 id="history-title" className="m-0 font-semibold text-lg tracking-[-0.02em]">
            {m.history_title()}
          </h2>
          <p className="m-0 mt-1 text-[13px] text-muted-foreground">
            {m.history_sub({ unit: UNIT_LABEL[unit], source, interval })}
          </p>
        </div>
      </div>
      <DayChips days={days} selected={day.key} onSelect={setKey} />
      {q.isPending ? (
        <div className="animate-pulse rounded-lg bg-muted" style={{ height: HEIGHT }} />
      ) : obs.length === 0 ? (
        <p className="m-0 py-10 text-center text-muted-foreground text-sm">
          {m.history_none()}
          {q.data?.oldest != null && day.to < q.data.oldest && (
            <> {m.history_since({ date: dayMonthOf(q.data.oldest, timeZone) })}</>
          )}
        </p>
      ) : (
        <>
          <dl className="m-0 grid grid-cols-2 gap-3 sm:grid-cols-4">
            <Figure label={m.history_maxWind()}>{speed(stats.maxWind)}</Figure>
            <Figure label={m.history_maxGust()}>{speed(stats.maxGust)}</Figure>
            <Figure label={m.history_average()}>{speed(stats.meanWind)}</Figure>
            <Figure label={m.history_mainDir()}>
              {stats.mainDir === null ? (
                '–'
              ) : (
                <span className="flex items-center gap-1.5">
                  <WindArrow deg={stats.mainDir} className="size-4" />
                  {compass(stats.mainDir)} · {Math.round(stats.mainDir)}°
                </span>
              )}
            </Figure>
          </dl>
          <Chart obs={obs} day={day} timeZone={timeZone} />
        </>
      )}
    </Card>
  );
}

function Figure({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-0.5">
      <dt className="text-muted-foreground text-xs">{label}</dt>
      <dd className="m-0 font-mono font-semibold text-[15px]">{children}</dd>
    </div>
  );
}

function DayChips({
  days,
  selected,
  onSelect,
}: {
  days: DayOption[];
  selected: string;
  onSelect: (key: string) => void;
}) {
  const fmt = new Intl.DateTimeFormat(intlLocale(), { weekday: 'short', day: 'numeric', timeZone: 'UTC' });
  // "Fr 9." / "Fri 9", without the comma Intl puts between them
  const short = (date: string) => {
    const p = Object.fromEntries(fmt.formatToParts(new Date(`${date}T12:00:00Z`)).map((x) => [x.type, x.value]));
    return `${p.weekday.replace(/\.$/, '')} ${p.day}${intlLocale().startsWith('de') ? '.' : ''}`;
  };
  const label = (d: DayOption, i: number) =>
    i === 0 ? m.history_today() : i === 1 ? m.history_yesterday() : short(d.date);
  return (
    <fieldset
      aria-label={m.history_days()}
      className="-mx-5 m-0 flex min-w-0 gap-1.5 overflow-x-auto border-0 px-5 py-0 [scrollbar-width:none]"
    >
      {days.map((d, i) => {
        const on = d.key === selected;
        return (
          <button
            key={d.key}
            type="button"
            aria-pressed={on}
            title={i === 0 ? m.history_last24() : undefined}
            onClick={() => onSelect(d.key)}
            className={cn(
              'flex h-10 shrink-0 cursor-pointer items-center rounded-full border px-3.5 font-semibold text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring/60',
              on ? 'border-primary bg-primary text-primary-foreground' : 'bg-background hover:border-border-strong',
            )}
          >
            {label(d, i)}
          </button>
        );
      })}
    </fieldset>
  );
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

function Chart({ obs, day, timeZone }: { obs: Observation[]; day: DayOption; timeZone: string }) {
  const [unit] = useUnit();
  const [theme] = useTheme();
  const box = useRef<HTMLDivElement>(null);
  const [hover, setHover] = useState<Observation | null>(null);
  const factor = unit === 'kn' ? 1 : unit === 'ms' ? MS_PER_KN : KMH_PER_KN;

  // biome-ignore lint/correctness/useExhaustiveDependencies: the colours come from the page, so a theme change redraws
  useEffect(() => {
    const el = box.current;
    if (!el) return;
    const fg = resolve('var(--muted-foreground)');
    const text = resolve('var(--foreground)');
    const grid = resolve('var(--line)');
    const gustColor = resolve('var(--chart-3)');
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
    const bands: uPlot.Plugin = {
      hooks: {
        drawClear: [
          (u) => {
            const { left, top: t, width, height } = u.bbox;
            const ctx = u.ctx;
            ctx.save();
            ctx.globalAlpha = dark ? 0.13 : 0.2;
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
            ctx.globalAlpha = 0.45;
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
            setHover(i == null ? null : (obs[i] ?? null));
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
      cursor: { drag: { x: false, y: false }, points: { size: 7 }, y: false },
      scales: { x: { time: false, range: [day.from, day.to] }, y: { range: [0, ymax] } },
      axes: [
        {
          stroke: fg,
          grid: { show: false },
          ticks: { show: true, stroke: grid, size: 4 },
          font: '11px Geist Mono Variable, monospace',
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
          font: '11px Geist Mono Variable, monospace',
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
          font: '11px Geist Mono Variable, monospace',
          size: 30,
          label: 'bft',
          labelSize: 14,
          labelFont: '11px Geist Mono Variable, monospace',
          splits: () => bftSplits,
          values: (_u, splits) => splits.map((_, i) => String(i + 1)),
        },
      ],
      series: [
        {},
        { label: m.history_mean(), stroke: text, width: 2.25, points: { show: false } },
        { label: m.history_gusts(), stroke: gustColor, width: 1.5, points: { show: false } },
      ],
      plugins: [bands],
    };
    const plot = new uPlot(opts, [x, mean, gust] as uPlot.AlignedData, el);
    const resize = new ResizeObserver(() => plot.setSize({ width: el.clientWidth, height: HEIGHT }));
    resize.observe(el);
    return () => {
      resize.disconnect();
      plot.destroy();
    };
  }, [obs, day, timeZone, factor, unit, theme]);

  const dayLabel = day.key === 'today' ? m.history_last24() : dayMonthOf(day.from + 12 * 3600, timeZone);
  return (
    <div className="flex flex-col gap-2">
      <div className="flex min-h-5 flex-wrap items-center gap-x-4 gap-y-1 text-xs" aria-live="polite">
        {hover ? (
          <span className="font-mono text-subtle">
            {clock(hover.at, timeZone)} · {m.history_mean()} {hover.wind === null ? '–' : formatWind(hover.wind, unit)}
            {hover.gust !== null && ` · ${m.history_gusts()} ${formatWind(hover.gust, unit)}`} {UNIT_LABEL[unit]}
            {hover.dir !== null && ` · ${compass(hover.dir)} ${Math.round(hover.dir)}°`}
          </span>
        ) : (
          <>
            <Legend color="var(--foreground)" label={m.history_mean()} thick />
            <Legend color="var(--chart-3)" label={m.history_gusts()} />
          </>
        )}
      </div>
      <div ref={box} role="img" aria-label={m.history_aria({ day: dayLabel })} className="w-full" />
    </div>
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
