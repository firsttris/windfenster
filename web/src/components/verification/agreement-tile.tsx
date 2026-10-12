import type { Forecast } from '@/bindings/Forecast';
import { AGREE_LABEL, modelColor, modelName } from '@/components/forecast/shared';
import { bestTime, spreadAt, stripTop } from '@/lib/agreement';
import { localClock } from '@/lib/forecast';
import { intlLocale } from '@/lib/i18n';
import { useUnit } from '@/lib/prefs';
import { formatWind, UNIT_LABEL } from '@/lib/units';
import { useNow } from '@/lib/use-now';
import { cn } from '@/lib/utils';
import { m } from '@/paraglide/messages';
import { Card, CardHeader, CardMeta, CardTitle } from '@/ui/card';

const TEXT = { high: 'text-agree-high', medium: 'text-agree-medium', low: 'text-agree-low' } as const;
/** Do the models agree at the best time ahead? Their spread, and each model on a strip from 0 to
 * 30 kn, or further in a storm. */
export function AgreementTile({ fc }: { fc: Forecast }) {
  const [unit] = useUnit();
  const now = useNow();
  const time = bestTime(fc, now);
  const s = time === null ? null : spreadAt(fc, time);
  const tz = fc.timezone ?? 'UTC';
  const slot = (() => {
    if (time === null) return '';
    const clock = localClock(tz);
    const day =
      clock(time).date === clock(now).date
        ? m.agree_today()
        : new Intl.DateTimeFormat(intlLocale(), { weekday: 'short', timeZone: tz }).format(new Date(time * 1000));
    return `${day} ${String(clock(time).hour).padStart(2, '0')}:00`;
  })();
  const top = stripTop(s?.values.map((v) => v.wind) ?? []);
  const pct = (kn: number) => `${Math.min(100, (kn / top) * 100).toFixed(1)}%`;
  return (
    <Card aria-labelledby="agree-title">
      <CardHeader>
        <CardTitle id="agree-title">{m.agree_title()}</CardTitle>
        {s && <CardMeta>{slot}</CardMeta>}
      </CardHeader>
      {!s ? (
        <p className="m-0 text-muted-foreground text-sm">{m.agree_none()}</p>
      ) : (
        <>
          <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
            <span className={cn('font-bold text-[42px] leading-none tracking-[-0.045em]', TEXT[s.level])}>
              {AGREE_LABEL[s.level]()}
            </span>
            <span className="text-muted-foreground text-sm">
              {m.agree_spread({
                spread: `${formatWind(s.spread, unit)} ${UNIT_LABEL[unit]}`,
                models: m.models_count({ count: s.values.length }),
              })}
            </span>
          </div>
          <div className="relative mt-1 h-[34px]" aria-hidden="true">
            <div className="absolute inset-x-0 top-4 h-0.5 bg-border" />
            <div
              className="absolute top-3 h-2.5 rounded-full bg-border-strong"
              style={{
                left: pct(Math.min(...s.values.map((v) => v.wind))),
                width: pct(s.spread),
              }}
            />
            {s.values.map((v) => (
              <span
                key={v.model}
                title={modelName(fc, v.model)}
                className="-ml-2 absolute top-[9px] size-4 rounded-full border-[3px] border-card"
                style={{ left: pct(v.wind), background: modelColor(fc, v.model) }}
              />
            ))}
          </div>
          <div className="flex justify-between font-mono text-[11px] text-dim" aria-hidden="true">
            {Array.from({ length: top / (top > 50 ? 20 : 10) + 1 }, (_, i) => i * (top > 50 ? 20 : 10)).map((kn) => (
              <span key={kn}>
                {formatWind(kn, unit)}
                {kn === top ? ` ${UNIT_LABEL[unit]}` : ''}
              </span>
            ))}
          </div>
          <ul className="m-0 grid list-none grid-cols-2 gap-x-3 gap-y-1.5 p-0 pt-1 sm:grid-cols-3">
            {s.values.map((v) => (
              <li key={v.model} className="flex items-center gap-1.5 text-subtle text-xs">
                <span className="size-2 shrink-0 rounded-full" style={{ background: modelColor(fc, v.model) }} />
                {modelName(fc, v.model)}{' '}
                <strong className="font-mono text-foreground">{formatWind(v.wind, unit)}</strong>
              </li>
            ))}
          </ul>
        </>
      )}
    </Card>
  );
}
