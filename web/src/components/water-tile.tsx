import { useQuery } from '@tanstack/react-query';
import { ArrowDown, ArrowRight, ArrowUp } from 'lucide-react';
import type { GaugeState } from '@/bindings/GaugeState';
import type { LevelPoint } from '@/bindings/LevelPoint';
import type { NearGauge } from '@/bindings/NearGauge';
import { gaugeQuery } from '@/lib/api';
import { clock, formatNumber, km } from '@/lib/format';
import { KMH_PER_KN, MS_PER_KN } from '@/lib/units';
import { cn } from '@/lib/utils';
import { levelTrend, type Trend } from '@/lib/water';
import { m } from '@/paraglide/messages';
import { Card, CardHeader, CardMeta, CardTitle } from '@/ui/card';

/** The gauge near a place (within 15 km): level and its week, flood state, flow, current. */
export function WaterTile({ lat, lon, timeZone }: { lat: number; lon: number; timeZone: string }) {
  const q = useQuery(gaugeQuery(lat, lon));
  if (!q.data) return null;
  return <Gauge g={q.data} timeZone={timeZone} />;
}

const TREND: Record<Trend, { label: () => string; Icon: typeof ArrowUp }> = {
  rising: { label: m.water_rising, Icon: ArrowUp },
  falling: { label: m.water_falling, Icon: ArrowDown },
  steady: { label: m.water_steady, Icon: ArrowRight },
};

/** The flood light from PEGELONLINE's own states only. */
function floodBadge(level: GaugeState, shipping: GaugeState) {
  if (shipping === 'high') return { text: m.water_aboveHsw(), tone: 'red' as const };
  if (level === 'high') return { text: m.water_flood(), tone: 'amber' as const };
  if (level === 'low') return { text: m.water_lowWater(), tone: 'green' as const };
  if (level === 'normal') return { text: m.water_noFlood(), tone: 'green' as const };
  return null;
}

const TONE = {
  green: { badge: 'border-success-border bg-success-surface text-success-foreground', dot: 'bg-success' },
  amber: { badge: 'border-warning-border bg-warning-surface text-warning-foreground', dot: 'bg-sun' },
  red: { badge: 'border-destructive/40 bg-destructive/10 text-destructive', dot: 'bg-destructive' },
};

function Gauge({ g, timeZone }: { g: NearGauge; timeZone: string }) {
  const { trend, yesterday } = levelTrend(g.history);
  const badge = floodBadge(g.levelState, g.shippingState);
  const currentKn = g.currentMs === null ? null : g.currentMs / MS_PER_KN;
  return (
    <Card aria-labelledby="water-title">
      <CardHeader className="flex-wrap">
        <CardTitle id="water-title" className="whitespace-nowrap">
          <svg
            viewBox="0 0 24 24"
            className="size-4 text-water"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            aria-hidden="true"
          >
            <path d="M2 8c2-1.5 4-1.5 6 0s4 1.5 6 0 4-1.5 6 0M2 13c2-1.5 4-1.5 6 0s4 1.5 6 0 4-1.5 6 0M2 18c2-1.5 4-1.5 6 0s4 1.5 6 0 4-1.5 6 0" />
          </svg>
          {g.water ? `${m.water_title()} · ${g.water}` : m.water_title()}
        </CardTitle>
        <CardMeta>{m.water_gauge({ name: g.name, distance: km(g.distanceKm) })}</CardMeta>
      </CardHeader>

      {g.level && (
        <div className="flex flex-wrap items-end justify-between gap-3">
          <div className="flex flex-col gap-1.5">
            <span className="text-muted-foreground text-xs">{m.water_level()}</span>
            <span className="flex items-baseline gap-1.5">
              <span className="font-mono font-semibold text-[52px] leading-[0.9] tracking-[-0.05em]">
                {formatNumber(g.level.value)}
              </span>
              <span className="text-muted-foreground">cm</span>
            </span>
          </div>
          <div className="flex flex-col items-start gap-1.5 sm:items-end">
            {badge && (
              <span
                className={cn(
                  'inline-flex items-center gap-1.5 rounded-full border px-2.5 py-1 font-semibold text-xs',
                  TONE[badge.tone].badge,
                )}
              >
                <span className={cn('size-[7px] rounded-full', TONE[badge.tone].dot)} aria-hidden="true" />
                {badge.text}
              </span>
            )}
            {trend && (
              <span className="inline-flex items-center gap-1 rounded-full border bg-secondary px-2.5 py-1 font-semibold text-subtle text-xs">
                {(() => {
                  const { Icon, label } = TREND[trend];
                  return (
                    <>
                      <Icon className="size-3" strokeWidth={3} aria-hidden="true" />
                      {label()}
                    </>
                  );
                })()}
                {yesterday !== null && ` · ${m.water_yesterday({ level: formatNumber(yesterday) })}`}
              </span>
            )}
          </div>
        </div>
      )}

      {g.history.length > 1 && <History points={g.history} />}

      {(g.flow || currentKn !== null || g.waterTemp) && (
        <div className="grid grid-cols-2 gap-2.5 border-t pt-3">
          {g.flow && (
            <Figure label={m.water_flow()}>
              {formatNumber(g.flow.value)} <Unit>m³/s</Unit>
            </Figure>
          )}
          {currentKn !== null && (
            <Figure
              label={
                <>
                  {m.water_current()} <span className="text-[11px] text-dim">{m.water_estimated()}</span>
                </>
              }
            >
              ≈ {formatNumber(currentKn, 1)} <Unit>kn</Unit>
              <span className="block font-normal text-muted-foreground text-xs">
                {formatNumber(currentKn * KMH_PER_KN, 1)} km/h
              </span>
            </Figure>
          )}
          {g.waterTemp && (
            <Figure label={m.water_temperature()}>
              {formatNumber(g.waterTemp.value, 1)} <Unit>°C</Unit>
            </Figure>
          )}
        </div>
      )}

      <span className="text-[11px] text-dim leading-snug">
        {m.water_source({ time: m.live_clock({ time: clock(g.level?.at ?? g.flow?.at ?? 0, timeZone) }) })}
        {currentKn !== null && ` · ${m.water_currentNote()}`}
      </span>
    </Card>
  );
}

function Figure({ label, children }: { label: React.ReactNode; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-0.5">
      <span className="text-muted-foreground text-xs">{label}</span>
      <span className="font-mono font-semibold text-lg">{children}</span>
    </div>
  );
}

function Unit({ children }: { children: React.ReactNode }) {
  return <span className="font-normal text-muted-foreground text-xs">{children}</span>;
}

/** The week as a line; the axis shows the range from lowest to highest. */
function History({ points }: { points: LevelPoint[] }) {
  const t0 = points[0].at;
  const t1 = points[points.length - 1].at;
  const lo = Math.min(...points.map((p) => p.cm));
  const hi = Math.max(...points.map((p) => p.cm));
  const span = Math.max(hi - lo, 4);
  const xy = points
    .map((p) => {
      const x = ((p.at - t0) / Math.max(t1 - t0, 1)) * 300;
      const y = 40 - ((p.cm - lo) / span) * 34;
      return `${x.toFixed(1)},${y.toFixed(1)}`;
    })
    .join(' ');
  return (
    <div className="flex flex-col gap-1">
      <svg
        viewBox="0 0 300 44"
        preserveAspectRatio="none"
        className="block h-11 w-full"
        role="img"
        aria-label={m.water_history({
          from: formatNumber(points[0].cm),
          to: formatNumber(points[points.length - 1].cm),
        })}
      >
        <line x1="0" y1="43" x2="300" y2="43" className="stroke-border" strokeWidth="1" />
        <polyline
          points={xy}
          fill="none"
          className="stroke-water"
          strokeWidth="2"
          strokeLinejoin="round"
          strokeLinecap="round"
          vectorEffect="non-scaling-stroke"
        />
      </svg>
      <div className="flex justify-between font-mono text-[11px] text-dim">
        <span>{m.water_weekAgo()}</span>
        <span>{m.water_now()}</span>
      </div>
    </div>
  );
}
