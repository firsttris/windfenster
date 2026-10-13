import { useQuery } from '@tanstack/react-query';
import { useMemo, useState } from 'react';
import type { Station } from '@/bindings/Station';
import { historyQuery } from '@/lib/api';
import { compass, dayMonthOf } from '@/lib/format';
import { type DayOption, dayOptions, dayStats } from '@/lib/history';
import { intlLocale } from '@/lib/i18n';
import { useUnit } from '@/lib/prefs';
import { formatWind, UNIT_LABEL } from '@/lib/units';
import { useNow } from '@/lib/use-now';
import { cn } from '@/lib/utils';
import { m } from '@/paraglide/messages';
import { Card } from '@/ui/card';
import { WindArrow } from '@/ui/wind-arrow';
import { HistoryChart } from './history-chart';

const HEIGHT = 240;

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
          <HistoryChart obs={obs} day={day} timeZone={timeZone} />
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
      className="relative -mx-5 m-0 flex min-w-0 gap-1.5 overflow-x-auto border-0 px-5 py-0 [scrollbar-width:none]"
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
