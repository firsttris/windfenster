/** The admin page's measurements: station lists, tracked stations, what came in lately, the
 * ten-year statistics, verification. */
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { X } from 'lucide-react';
import { useState } from 'react';
import type { AdminMeasurements } from '@/bindings/AdminMeasurements';
import { Failed } from '@/components/page';
import { span } from '@/lib/admin';
import { adminMeasurementsQuery } from '@/lib/admin-api';
import { send } from '@/lib/api';
import { ago, formatNumber } from '@/lib/format';
import { describeError } from '@/lib/i18n';
import { cn } from '@/lib/utils';
import { m } from '@/paraglide/messages';
import { Button } from '@/ui/button';
import { Card } from '@/ui/card';
import { Chip, Row, TabHead } from './parts';
import { FIELD, useSettings } from './setting';

export function MeasurementsTab() {
  const q = useQuery(adminMeasurementsQuery);
  if (q.isError) return <Failed error={q.error} />;
  if (!q.data) return <div aria-busy="true" className="h-96 animate-pulse rounded-xl bg-muted" />;
  return (
    <>
      <TabHead title={m.admin_tabMeasurements()} lead={m.admin_measurementsLead()} />
      <div className="grid grid-cols-[repeat(auto-fit,minmax(340px,1fr))] items-start gap-5">
        <ListsCard data={q.data} />
        <TrackedCard data={q.data} />
      </div>
      <FeedsCard data={q.data} />
      <div className="grid grid-cols-[repeat(auto-fit,minmax(340px,1fr))] items-start gap-5">
        <StatsCard data={q.data} />
        <VerificationCard data={q.data} />
      </div>
    </>
  );
}

/** Runs a task now, or sends another request; says how it went. */
function useRun() {
  const queryClient = useQueryClient();
  const [message, setMessage] = useState<string | null>(null);
  const run = useMutation({
    mutationFn: (path: string) => send<void>('POST', path),
    onSuccess: () => {
      setMessage(m.admin_asked());
      setTimeout(() => void queryClient.invalidateQueries({ queryKey: ['admin'] }), 2000);
    },
    onError: (e) => setMessage(describeError(e)),
  });
  return { run: (path: string) => run.mutate(path), pending: run.isPending, message };
}

const LIST_NAME: Record<string, [() => string, () => string]> = {
  metar: [m.admin_listMetar, m.admin_listMetarNote],
  dwd: [m.admin_listDwd, m.admin_listDwdNote],
  owm: [m.admin_listOwm, () => ''],
};

function ListsCard({ data }: { data: AdminMeasurements }) {
  const r = useRun();
  return (
    <Card className="gap-3">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h2 className="m-0 font-semibold text-base">{m.admin_lists()}</h2>
        <Button size="sm" disabled={r.pending} onClick={() => r.run('/admin/tasks/stations/run')}>
          {m.admin_importNow()}
        </Button>
      </div>
      {data.lists.map((l) => {
        const [name, note] = LIST_NAME[l.source] ?? [() => l.source, () => ''];
        return (
          <div
            key={l.source}
            className="flex items-center justify-between gap-3 border-line border-b pb-2.5 text-[13px]"
          >
            <span className="flex flex-col">
              <span className="font-medium">{name()}</span>
              <span className="text-muted-foreground text-xs">
                {[note(), l.seenAt > 0 ? m.admin_listSeen({ ago: ago(l.seenAt) }) : ''].filter(Boolean).join(' · ')}
              </span>
            </span>
            <span className="font-mono">{formatNumber(l.stations)}</span>
          </div>
        );
      })}
      {r.message && <p className="m-0 text-muted-foreground text-sm">{r.message}</p>}
    </Card>
  );
}

function TrackedCard({ data }: { data: AdminMeasurements }) {
  const settings = useSettings();
  const [slug, setSlug] = useState('');
  const list = settings.data?.settings.trackedStations ?? [];
  const lockedBy = settings.locked('trackedStations');
  const save = (next: string[]) => settings.set('trackedStations', next);
  return (
    <Card className="gap-3">
      <h2 className="m-0 font-semibold text-base">{m.admin_tracked()}</h2>
      <p className="m-0 text-muted-foreground text-[13px] leading-relaxed">{m.admin_trackedText()}</p>
      <ul className="m-0 flex list-none flex-wrap gap-2 p-0">
        {data.tracked.map((t) => (
          <li key={t.slug}>
            <span
              className={cn(
                'inline-flex items-center gap-1 rounded-full border py-0.5 pr-1 pl-2.5 text-[13px]',
                t.name ? 'bg-secondary' : 'border-warning-border bg-warning-surface text-warning-foreground',
              )}
            >
              {t.name ? `${t.code ?? t.slug.toUpperCase()} · ${t.name}` : t.slug}
              {!lockedBy && (
                <button
                  type="button"
                  aria-label={m.admin_removeX({ name: t.name ?? t.slug })}
                  disabled={settings.pending}
                  onClick={() => save(list.filter((s) => s !== t.slug))}
                  className="inline-flex size-6 cursor-pointer items-center justify-center rounded-md hover:bg-background"
                >
                  <X className="size-3.5" />
                </button>
              )}
            </span>
          </li>
        ))}
      </ul>
      {data.tracked
        .filter((t) => !t.name)
        .map((t) => (
          <span key={t.slug} className="text-warning-foreground text-xs">
            {m.admin_trackedUnknown({ slug: t.slug })}
          </span>
        ))}
      {lockedBy ? (
        <span className="text-muted-foreground text-xs">{m.admin_locked({ name: lockedBy })}</span>
      ) : (
        <form
          className="flex gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            const s = slug.trim().toLowerCase();
            if (s && !list.includes(s)) save([...list, s]);
            setSlug('');
          }}
        >
          <input
            aria-label={m.admin_trackedAdd()}
            placeholder={m.admin_trackedAdd()}
            value={slug}
            onChange={(e) => setSlug(e.target.value)}
            className={cn(FIELD, 'flex-1 font-sans')}
          />
          <Button type="submit" size="sm" variant="outline" disabled={settings.pending || !slug.trim()}>
            {m.admin_add()}
          </Button>
        </form>
      )}
      {settings.error && <p className="m-0 text-destructive text-sm">{settings.error}</p>}
    </Card>
  );
}

function FeedsCard({ data }: { data: AdminMeasurements }) {
  const c = data.counts;
  const feeds: [string, string, string?][] = [
    [m.admin_feedMetar(), formatNumber(c.metarHour)],
    [m.admin_feedDwd(), formatNumber(c.dwdHour)],
    [m.admin_feedTaf(), formatNumber(c.tafs)],
    [m.admin_feedGauges(), formatNumber(c.gaugesFresh), m.admin_ofTotal({ total: formatNumber(c.gauges) })],
  ];
  return (
    <Card className="gap-3.5">
      <h2 className="m-0 font-semibold text-base">{m.admin_feeds()}</h2>
      <div className="grid grid-cols-[repeat(auto-fit,minmax(200px,1fr))] gap-2.5">
        {feeds.map(([label, value, sub]) => (
          <div key={label} className="flex flex-col gap-1.5 rounded-xl border border-line bg-background p-3.5">
            <span className="font-bold text-2xl tracking-[-0.04em]">{value}</span>
            <span className="text-muted-foreground text-xs">
              {label}
              {sub && ` ${sub}`}
            </span>
          </div>
        ))}
      </div>
    </Card>
  );
}

function StatsCard({ data }: { data: AdminMeasurements }) {
  const r = useRun();
  return (
    <Card className="gap-3">
      <h2 className="m-0 font-semibold text-base">{m.admin_stats()}</h2>
      <span className="text-sm">{m.admin_statsMade({ count: formatNumber(data.counts.windStats) })}</span>
      {data.stats.length === 0 && <span className="text-muted-foreground text-sm">{m.admin_statsNone()}</span>}
      {data.stats.map((s) => (
        <div
          key={s.station}
          className="flex items-center justify-between gap-3 border-line border-t pt-2.5 text-[13px]"
        >
          <span className="flex min-w-0 flex-col gap-0.5">
            <span className="font-medium">{s.slug?.toUpperCase() ?? `#${s.station}`}</span>
            {s.error && (
              <span className="truncate text-muted-foreground text-xs" title={s.error}>
                {s.error}
              </span>
            )}
          </span>
          {s.running ? (
            <Chip tone="running">{m.admin_statsRunning()}</Chip>
          ) : (
            <span className="flex items-center gap-2">
              <Chip tone="error">{m.admin_statsFailed({ ago: span(s.failedAgo ?? 0) })}</Chip>
              <Button
                size="sm"
                variant="outline"
                disabled={r.pending}
                onClick={() => r.run(`/admin/stats/${s.station}/retry`)}
              >
                {m.admin_retry()}
              </Button>
            </span>
          )}
        </div>
      ))}
      {r.message && <p className="m-0 text-muted-foreground text-sm">{r.message}</p>}
    </Card>
  );
}

/** yyyymmdd as a date */
function dayText(day: number): string {
  const s = String(day);
  return `${s.slice(6, 8)}.${s.slice(4, 6)}.${s.slice(0, 4)}`;
}

function VerificationCard({ data }: { data: AdminMeasurements }) {
  const r = useRun();
  const c = data.counts;
  return (
    <Card className="gap-3">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h2 className="m-0 font-semibold text-base">{m.admin_verification()}</h2>
        <Button size="sm" variant="outline" disabled={r.pending} onClick={() => r.run('/admin/tasks/purge/run')}>
          {m.admin_verifyNow()}
        </Button>
      </div>
      <Row label={m.admin_verifiedDay()}>
        {c.verifiedDay ? `${dayText(c.verifiedDay)} · ${m.admin_verifiedStations({ count: c.verifiedStations })}` : '–'}
      </Row>
      <Row label={m.admin_keep()}>
        <span className="text-xs">{m.admin_keepText()}</span>
      </Row>
      {r.message && <p className="m-0 text-muted-foreground text-sm">{r.message}</p>}
    </Card>
  );
}
