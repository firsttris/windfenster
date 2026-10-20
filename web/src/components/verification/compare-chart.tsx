import 'uplot/dist/uPlot.min.css';
import { useEffect, useMemo, useRef } from 'react';
import uPlot from 'uplot';
import type { Forecast } from '@/bindings/Forecast';
import type { Observation } from '@/bindings/Observation';
import { modelName } from '@/components/forecast/shared';
import { compareData } from '@/lib/compare';
import { MIX } from '@/lib/forecast';
import { modelColor } from '@/lib/model-color';
import { useTheme, useUnit } from '@/lib/prefs';
import { UNIT_LABEL } from '@/lib/units';
import { useNow } from '@/lib/use-now';
import { cn } from '@/lib/utils';
import { m } from '@/paraglide/messages';
import { Card } from '@/ui/card';

/** A CSS colour as the canvas needs it: var(--x) resolved against the page. */
function resolve(color: string): string {
  const v = color.match(/^var\((--[\w-]+)\)$/);
  return v ? getComputedStyle(document.documentElement).getPropertyValue(v[1]).trim() : color;
}

/** Today's forecast of every model from 7 to 19 o'clock, with the measurements so far
 * (uPlot). The chosen model is drawn strong. */
export function CompareChart({
  fc,
  selected,
  onSelect,
  observations,
  source,
}: {
  fc: Forecast;
  selected: string | null;
  onSelect: (id: string) => void;
  observations: readonly Observation[];
  /** Where the measurements come from, e.g. "METAR EDSB"; null without any */
  source: string | null;
}) {
  const [unit] = useUnit();
  const [theme] = useTheme();
  const now = useNow();
  const data = useMemo(() => compareData(fc, observations, now, unit), [fc, observations, now, unit]);
  const box = useRef<HTMLDivElement>(null);
  const clock = fc.timezone ?? 'UTC';

  // biome-ignore lint/correctness/useExhaustiveDependencies: the colours come from the page, so a theme change redraws
  useEffect(() => {
    const el = box.current;
    if (!el || !data) return;
    const fg = resolve('var(--muted-foreground)');
    const grid = resolve('var(--line)');
    const measuredColor = resolve('var(--measured)');
    const card = resolve('var(--card)');
    const series: uPlot.Series[] = [
      {},
      ...data.models.map((id) => {
        const on = id === selected;
        return {
          label: modelName(fc, id),
          stroke: resolve(modelColor(fc, id)),
          width: on ? 3.5 : id === MIX ? 2 : 1.75,
          dash: id === MIX && !on ? [6, 5] : undefined,
          alpha: on || !selected ? 1 : 0.55,
          spanGaps: true,
          points: { show: false },
        } satisfies uPlot.Series;
      }),
      {
        label: m.chart_measured(),
        stroke: measuredColor,
        fill: measuredColor,
        paths: () => null,
        points: { show: true, size: 9, width: 2.5, stroke: card, fill: measuredColor },
      },
    ];
    const hourFmt = new Intl.DateTimeFormat('en-GB', { hour: '2-digit', hourCycle: 'h23', timeZone: clock });
    const nowLine: uPlot.Plugin = {
      hooks: {
        draw: [
          (u) => {
            if (now < data.from || now > data.to) return;
            const x = u.valToPos(now, 'x', true);
            const { top, height } = u.bbox;
            const ctx = u.ctx;
            ctx.save();
            ctx.strokeStyle = resolve('var(--foreground)');
            ctx.globalAlpha = 0.5;
            ctx.setLineDash([4, 4]);
            ctx.lineWidth = devicePixelRatio;
            ctx.beginPath();
            ctx.moveTo(x, top);
            ctx.lineTo(x, top + height);
            ctx.stroke();
            ctx.globalAlpha = 1;
            ctx.fillStyle = resolve('var(--subtle)');
            ctx.font = `${11 * devicePixelRatio}px Geist Variable, sans-serif`;
            ctx.fillText(m.chart_now(), x + 6 * devicePixelRatio, top + 12 * devicePixelRatio);
            ctx.restore();
          },
        ],
      },
    };
    const opts: uPlot.Options = {
      width: el.clientWidth,
      height: 230,
      legend: { show: false },
      cursor: { drag: { x: false, y: false }, points: { size: 7 } },
      scales: { x: { time: false }, y: { range: (_u, _min, max) => [0, Math.max(10, Math.ceil((max + 2) / 5) * 5)] } },
      axes: [
        {
          stroke: fg,
          grid: { show: false },
          ticks: { show: false },
          font: '11px Geist Mono Variable, monospace',
          splits: () => {
            const out: number[] = [];
            for (let t = data.from; t <= data.to; t += 2 * 3600) out.push(t);
            return out;
          },
          values: (_u, splits) => splits.map((t) => hourFmt.format(new Date(t * 1000))),
        },
        {
          stroke: fg,
          grid: { stroke: grid, width: 1 },
          ticks: { show: false },
          font: '11px Geist Mono Variable, monospace',
          size: 36,
        },
      ],
      series,
      plugins: [nowLine],
    };
    const plot = new uPlot(opts, [data.x, ...data.lines, data.measured] as uPlot.AlignedData, el);
    const resize = new ResizeObserver(() => plot.setSize({ width: el.clientWidth, height: 230 }));
    resize.observe(el);
    return () => {
      resize.disconnect();
      plot.destroy();
    };
  }, [data, fc, selected, now, clock, theme]);

  if (!data) return null;
  return (
    <Card aria-labelledby="compare-title" className="min-w-0 flex-[999_1_620px]">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div>
          <h2 id="compare-title" className="m-0 font-semibold text-lg tracking-[-0.02em]">
            {m.chart_title()}
          </h2>
          <p className="m-0 mt-1 text-[13px] text-muted-foreground">
            {source && data.measured.some((v) => v !== null)
              ? m.chart_sub({ unit: UNIT_LABEL[unit], source })
              : m.chart_subPlain({ unit: UNIT_LABEL[unit] })}
          </p>
        </div>
        <div className="flex flex-wrap gap-x-3.5 gap-y-1.5">
          {data.models.map((id) => (
            <button
              key={id}
              type="button"
              onClick={() => onSelect(id)}
              aria-pressed={id === selected}
              className={cn(
                'flex cursor-pointer items-center gap-1.5 rounded-sm text-xs outline-none focus-visible:ring-2 focus-visible:ring-ring/60',
                id === selected ? 'font-semibold text-foreground' : 'text-muted-foreground hover:text-foreground',
              )}
            >
              <span className="h-[3px] w-3.5 rounded-full" style={{ background: modelColor(fc, id) }} />
              {modelName(fc, id)}
            </button>
          ))}
        </div>
      </div>
      <div
        ref={box}
        role="img"
        aria-label={`${m.chart_aria()}${source ? ` · ${m.chart_sub({ unit: UNIT_LABEL[unit], source })}` : ''}`}
        className="w-full"
      />
    </Card>
  );
}
