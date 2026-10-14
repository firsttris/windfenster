/** The admin page's overview: what needs a decision, the day's numbers, every background task and
 * every outside source. */
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Link } from '@tanstack/react-router';
import { TriangleAlert } from 'lucide-react';
import type { ReactNode } from 'react';
import type { Outcome } from '@/bindings/Outcome';
import type { SourceStatus } from '@/bindings/SourceStatus';
import type { TaskStatus } from '@/bindings/TaskStatus';
import type { Warning } from '@/bindings/Warning';
import { Failed } from '@/components/page';
import { type AdminTab, bytes, cadence, percent, span, took, diskUsed as used_ } from '@/lib/admin';
import { adminOverviewQuery } from '@/lib/admin-api';
import { send } from '@/lib/api';
import { ago, formatNumber } from '@/lib/format';
import { m } from '@/paraglide/messages';
import { Button } from '@/ui/button';
import { Card } from '@/ui/card';
import { sourceName, taskName } from './names';
import { Chip, ROW, TabHead, Table, type Tone } from './parts';

export function OverviewTab() {
  const o = useQuery(adminOverviewQuery);
  if (o.isError) return <Failed error={o.error} />;
  if (!o.data) return <div aria-busy="true" className="h-96 animate-pulse rounded-xl bg-muted" />;
  const d = o.data;
  const now = Date.now() / 1000;
  const used = d.budget.refresh + d.budget.opened;
  const diskUsed = d.disk ? used_(d.disk) : 0;
  return (
    <>
      <TabHead title={m.admin_tabOverview()} lead={m.admin_overviewLead()}>
        <span className="font-mono text-muted-foreground text-xs">
          {m.admin_runningSince({ time: span(now - d.startedAt), schema: d.schemaVersion, version: d.version })}
        </span>
      </TabHead>
      <section aria-label={m.admin_overviewLead()} className="flex flex-col gap-2">
        {d.warnings.length === 0 && <p className="m-0 text-muted-foreground text-sm">{m.admin_noWarnings()}</p>}
        {d.warnings.map((w) => (
          <WarningBox key={JSON.stringify(w)} warning={w} />
        ))}
      </section>
      <section className="grid grid-cols-[repeat(auto-fit,minmax(190px,1fr))] gap-3">
        <Kpi
          label={m.admin_kpiBudget()}
          value={formatNumber(used)}
          unit={
            d.budget.limit === null
              ? m.admin_kpiBudgetFree()
              : m.admin_kpiBudgetOf({ limit: formatNumber(d.budget.limit) })
          }
          share={d.budget.limit ? percent(used, d.budget.limit) : 0}
          sub={d.budget.openedLimit !== null ? m.admin_kpiBudgetSub({ limit: formatNumber(d.budget.openedLimit) }) : ''}
          bar="bg-chart-1"
        />
        <Kpi
          label={m.admin_kpiDisk()}
          value={d.disk ? bytes(diskUsed) : '–'}
          share={d.disk ? percent(diskUsed, diskUsed + d.disk.free) : 0}
          sub={d.disk ? m.admin_kpiDiskSub({ free: bytes(d.disk.free) }) : ''}
          bar="bg-foreground"
        />
        <Kpi
          label={m.admin_kpiUsers()}
          value={formatNumber(d.users.total)}
          share={percent(d.users.activeMonth, d.users.total)}
          sub={m.admin_kpiUsersSub({ month: d.users.activeMonth, week: d.users.newWeek })}
          bar="bg-chart-4"
        />
        <Kpi
          label={m.admin_kpiAlarms()}
          value={formatNumber(d.alarms.push + d.alarms.email)}
          share={percent(d.alarms.push + d.alarms.email, d.alarms.push + d.alarms.email + d.alarms.failed)}
          sub={m.admin_kpiAlarmsSub({ push: d.alarms.push, email: d.alarms.email, failed: d.alarms.failed })}
          bar="bg-chart-3"
        />
      </section>
      <Tasks tasks={d.tasks} now={now} />
      <Sources sources={d.sources} now={now} />
    </>
  );
}

function Kpi(p: { label: string; value: string; unit?: string; share: number; sub: string; bar: string }) {
  return (
    <Card className="gap-2 px-[18px] py-4">
      <h2 className="m-0 font-medium text-[13px] text-muted-foreground">{p.label}</h2>
      <div className="flex items-baseline gap-1.5">
        <span className="font-bold text-[28px] tracking-[-0.04em]">{p.value}</span>
        {p.unit && <span className="font-mono text-muted-foreground text-xs">{p.unit}</span>}
      </div>
      <div className="h-1.5 overflow-hidden rounded-full bg-line">
        <div className={`h-full rounded-full ${p.bar}`} style={{ width: `${p.share}%` }} />
      </div>
      <span className="text-muted-foreground text-xs">{p.sub}</span>
    </Card>
  );
}

/** What a warning says, and which tab shows more. */
function warningText(w: Warning): { title: string; text: string; tab: AdminTab; tone: 'warn' | 'error' } {
  switch (w.code) {
    case 'public_url_missing':
      return {
        title: m.admin_warnPublicUrlMissing(),
        text: m.admin_warnPublicUrlMissingText(),
        tab: 'system',
        tone: 'warn',
      };
    case 'public_url_not_https':
      return {
        title: m.admin_warnPublicUrlNotHttps(),
        text: m.admin_warnPublicUrlNotHttpsText(),
        tab: 'system',
        tone: 'warn',
      };
    case 'ip_header_missing':
      return {
        title: m.admin_warnIpHeader(),
        text: m.admin_warnIpHeaderText({ count: w.count }),
        tab: 'system',
        tone: 'warn',
      };
    case 'task_failed':
      return {
        title: m.admin_warnTaskFailed({ task: taskName(w.task) }),
        text: m.admin_warnTaskFailedText(),
        tab: 'overview',
        tone: 'error',
      };
    case 'source_failing':
      return {
        title: m.admin_warnSource({ source: sourceName(w.source) }),
        text: m.admin_warnSourceText({ errors: w.errors }),
        tab: 'overview',
        tone: 'warn',
      };
    case 'loop_panicked':
      return {
        title: m.admin_warnPanic({ name: taskName(w.name) }),
        text: m.admin_warnPanicText({ count: w.count }),
        tab: 'system',
        tone: 'error',
      };
    case 'budget_high':
      return {
        title: m.admin_warnBudget({ percent: w.percent }),
        text: m.admin_warnBudgetText(),
        tab: 'forecasts',
        tone: 'warn',
      };
    case 'new_version':
      return {
        title: m.admin_warnNewVersion({ version: w.version }),
        text: m.admin_warnNewVersionText(),
        tab: 'system',
        tone: 'warn',
      };
    case 'basemap_waiting':
      return { title: m.admin_warnBasemap(), text: m.admin_warnBasemapText(), tab: 'map', tone: 'warn' };
    case 'legal_missing':
      return { title: m.admin_warnLegal(), text: m.admin_warnLegalText(), tab: 'system', tone: 'warn' };
    case 'disk_low':
      return {
        title: m.admin_warnDisk({ free: bytes(w.free) }),
        text: m.admin_warnDiskText(),
        tab: 'storage',
        tone: 'warn',
      };
  }
}

function WarningBox({ warning }: { warning: Warning }) {
  const t = warningText(warning);
  return (
    <div
      className={
        t.tone === 'error'
          ? 'flex flex-wrap items-start gap-3 rounded-xl border border-destructive/40 bg-destructive/10 px-4 py-3.5 text-destructive'
          : 'flex flex-wrap items-start gap-3 rounded-xl border border-warning-border bg-warning-surface px-4 py-3.5 text-warning-foreground'
      }
    >
      <TriangleAlert aria-hidden="true" className="mt-0.5 size-[18px] shrink-0" />
      <div className="flex flex-[1_1_300px] flex-col gap-0.5">
        <strong className="font-semibold text-sm">{t.title}</strong>
        <span className="text-[13px] leading-snug opacity-85">{t.text}</span>
      </div>
      <Button size="sm" asChild className="self-center">
        <Link to="/admin" search={{ tab: t.tab }}>
          {m.admin_look()}
        </Link>
      </Button>
    </div>
  );
}

const OUTCOME: Record<Outcome, { tone: Tone; label: () => string }> = {
  waiting: { tone: 'off', label: m.admin_outcomeWaiting },
  running: { tone: 'running', label: m.admin_outcomeRunning },
  ok: { tone: 'ok', label: m.admin_outcomeOk },
  partly: { tone: 'warn', label: m.admin_outcomePartly },
  failed: { tone: 'error', label: m.admin_outcomeFailed },
};

export function every(seconds: number): string {
  const c = cadence(seconds);
  if (c.unit === 'week') return m.admin_everyWeek();
  if (c.unit === 'day') return m.admin_everyDay();
  if (c.unit === 'hour') return m.admin_everyHour();
  return m.admin_everyMin({ n: c.n });
}

function Tasks({ tasks, now }: { tasks: TaskStatus[]; now: number }) {
  return (
    <Card className="gap-3.5">
      <h2 className="m-0 font-semibold text-base">{m.admin_tasks()}</h2>
      <Table
        cols="2.1fr 0.8fr 0.9fr 0.7fr 2.4fr 0.9fr"
        head={[
          m.admin_colTask(),
          m.admin_colEvery(),
          m.admin_colLast(),
          m.admin_colTook(),
          m.admin_colResult(),
          <span key="a" className="block text-right">
            {m.admin_colAction()}
          </span>,
        ]}
      >
        {tasks.map((t) => (
          <TaskRow key={t.id} task={t} now={now} />
        ))}
      </Table>
    </Card>
  );
}

function TaskRow({ task, now }: { task: TaskStatus; now: number }) {
  const queryClient = useQueryClient();
  const run = useMutation({
    mutationFn: () => send<void>('POST', `/admin/tasks/${task.id}/run`),
    onSuccess: () => setTimeout(() => void queryClient.invalidateQueries({ queryKey: ['admin', 'overview'] }), 1500),
  });
  const o = OUTCOME[task.outcome];
  const note: ReactNode = task.error ?? (task.count !== null ? m.admin_count({ count: formatNumber(task.count) }) : '');
  return (
    <div className={ROW}>
      <span className="font-medium">{taskName(task.id)}</span>
      {task.id === 'basemap' ? (
        // The loop checks daily; the big downloads come after 120 days or a year (service/basemap.rs)
        <span className="font-mono text-subtle text-xs" title={m.admin_checksDailyHint()}>
          {m.admin_checksDaily()}
        </span>
      ) : (
        <span className="font-mono text-subtle text-xs">{every(task.everyS)}</span>
      )}
      <span className="font-mono text-subtle text-xs">{task.startedAt ? ago(task.startedAt, now) : '–'}</span>
      <span className="font-mono text-muted-foreground text-xs">{task.tookMs !== null ? took(task.tookMs) : '–'}</span>
      <span className="flex min-w-0 items-center gap-2">
        <Chip tone={o.tone}>{o.label()}</Chip>
        <span className="truncate text-muted-foreground text-xs" title={typeof note === 'string' ? note : undefined}>
          {task.panics > 0 ? `${m.admin_panics({ count: task.panics })} · ` : ''}
          {note}
        </span>
      </span>
      <span className="text-right">
        <Button
          size="sm"
          variant="outline"
          disabled={run.isPending || run.isSuccess || task.outcome === 'running'}
          onClick={() => run.mutate()}
        >
          {run.isSuccess ? m.admin_asked() : m.admin_runNow()}
        </Button>
      </span>
    </div>
  );
}

function sourceTone(s: SourceStatus): Tone {
  if (s.errorsDay === 0) return 'ok';
  if (s.okAt !== null && (s.failedAt === null || s.okAt > s.failedAt)) return 'warn';
  return 'error';
}

const DOT: Record<Tone, string> = {
  ok: 'bg-success',
  warn: 'bg-agree-medium',
  error: 'bg-destructive',
  running: 'bg-chart-1',
  off: 'bg-dim',
};

function Sources({ sources, now }: { sources: SourceStatus[]; now: number }) {
  return (
    <Card className="gap-3.5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h2 className="m-0 font-semibold text-base">{m.admin_sources()}</h2>
        <span className="text-muted-foreground text-xs">{m.admin_sourcesHint()}</span>
      </div>
      <div className="grid grid-cols-[repeat(auto-fill,minmax(220px,1fr))] gap-2.5">
        {sources.map((s) => (
          <div key={s.id} className="flex flex-col gap-1.5 rounded-xl border border-line bg-background px-3.5 py-3">
            <div className="flex items-center gap-2">
              <span className={`size-2 shrink-0 rounded-full ${DOT[sourceTone(s)]}`} />
              <span className="font-semibold text-sm">{sourceName(s.id)}</span>
            </div>
            <span className="font-mono text-subtle text-xs">
              {s.okAt ? ago(s.okAt, now) : m.admin_sourceNever()} · {m.admin_sourceErrors({ count: s.errorsDay })}
            </span>
            {s.lastError && s.errorsDay > 0 && (
              <span className="truncate text-muted-foreground text-xs" title={s.lastError}>
                {s.lastError}
              </span>
            )}
          </div>
        ))}
      </div>
    </Card>
  );
}
