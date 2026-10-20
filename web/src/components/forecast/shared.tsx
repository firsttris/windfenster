/** Pieces the desktop table and the phone list share. */
import type { Forecast } from '@/bindings/Forecast';
import type { WindUnit } from '@/bindings/WindUnit';
import { dayAgreement } from '@/lib/agreement';
import { type Day, MIX, type Stage, weatherOf } from '@/lib/forecast';
import { formatWind, UNIT_LABEL } from '@/lib/units';
import { cn } from '@/lib/utils';
import { m } from '@/paraglide/messages';
import { WeatherIcon } from '@/ui/weather-icon';
import { WIND_SCALE } from '@/ui/wind-scale';

export function modelName(fc: Forecast, id: string): string {
  if (id === MIX) return m.mix_name();
  if (id === 'taf') return 'TAF';
  return fc.models.find((x) => x.id === id)?.name ?? id;
}

export function stageLabel(fc: Forecast, day: Day): string {
  const cls = fc.models.find((x) => x.id === day.source)?.class;
  const stages: Record<Stage, () => string> = {
    hourly: () => m.stage_hourly({ label: cls === 'super' ? m.class_super() : m.class_global() }),
    threeHourly: m.stage_threeHourly,
    trend: m.stage_trend,
  };
  return stages[day.stage]();
}

const AGREE_LABEL = { high: m.agree_high, medium: m.agree_medium, low: m.agree_low } as const;
const AGREE_DOT = { high: 'bg-agree-high', medium: 'bg-agree-medium', low: 'bg-agree-low' } as const;

/** How far apart the models are at the day's windiest hour, as a dot. */
export function AgreementDot({ fc, day }: { fc: Forecast; day: Day }) {
  const a = dayAgreement(
    fc,
    day.cells.map((c) => c.time),
  );
  if (!a) return null;
  const text = m.agree_day({ level: AGREE_LABEL[a.level]() });
  // An sr-only text would be positioned outside the table's scroll area; a label is not.
  return <span role="img" aria-label={text} title={text} className={cn('size-2 rounded-full', AGREE_DOT[a.level])} />;
}

export { AGREE_DOT, AGREE_LABEL };

export function StageChip({ fc, day }: { fc: Forecast; day: Day }) {
  return (
    <span
      className={cn(
        'whitespace-nowrap rounded-full px-2 py-0.5 font-semibold text-[11px]',
        day.stage === 'hourly' && 'bg-primary text-primary-foreground',
        day.stage === 'threeHourly' && 'border border-border-strong text-subtle',
        day.stage === 'trend' && 'border border-border-strong border-dashed text-muted-foreground',
      )}
    >
      {stageLabel(fc, day)}
    </span>
  );
}

const WEATHER_LABEL = {
  sun: m.weather_sun,
  partly: m.weather_partly,
  cloud: m.weather_cloud,
  rain: m.weather_rain,
} as const;

export function Weather({ cloud, rain, className }: { cloud: number | null; rain: number | null; className?: string }) {
  const w = weatherOf(cloud, rain);
  return w ? (
    <WeatherIcon weather={w} label={WEATHER_LABEL[w]()} className={className} />
  ) : (
    <span className="text-dim">–</span>
  );
}

/** The colour scale with its edges in the chosen unit. */
export function ScaleLegend({ unit }: { unit: WindUnit }) {
  return (
    <div className="flex items-center gap-2.5">
      <span className="text-muted-foreground text-xs">{m.scale_windIn({ unit: UNIT_LABEL[unit] })}</span>
      <div className="flex overflow-hidden rounded-[7px]">
        {/* Every other step: fourteen marks for the scale's 27 */}
        {WIND_SCALE.filter((_, i) => i % 2 === 0).map((b, i, shown) => (
          <span
            key={b.from}
            className="px-1 py-1 font-mono font-semibold text-[10px] sm:px-1.5 sm:text-[11px]"
            style={{ background: b.bg, color: b.fg }}
          >
            {formatWind(b.from, unit)}
            {i === shown.length - 1 ? '+' : ''}
          </span>
        ))}
      </div>
    </div>
  );
}
