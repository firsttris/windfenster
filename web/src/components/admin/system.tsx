/** The admin page's system: the version and whether a newer one is out, the environment, the
 * caches, what visitors see of the operator (notice, legal notice, privacy policy) and the log. */
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { Failed } from '@/components/page';
import { adminLogLinesQuery, adminLogQuery, adminSystemQuery } from '@/lib/admin-api';
import { send } from '@/lib/api';
import { clock, dayMonthOf, formatNumber } from '@/lib/format';
import { m } from '@/paraglide/messages';
import { Button } from '@/ui/button';
import { Card } from '@/ui/card';
import { Segmented } from '@/ui/segmented';
import { Chip, TabHead } from './parts';
import { TextField, useSettings } from './setting';

export function SystemTab() {
  const q = useQuery(adminSystemQuery);
  if (q.isError) return <Failed error={q.error} />;
  if (!q.data) return <div aria-busy="true" className="h-96 animate-pulse rounded-xl bg-muted" />;
  const d = q.data;
  return (
    <>
      <TabHead title={m.admin_tabSystem()} lead={m.admin_systemLead()} />
      <div className="grid grid-cols-[repeat(auto-fit,minmax(340px,1fr))] items-start gap-5">
        <div className="flex flex-col gap-5">
          <Card className="gap-3">
            <h2 className="m-0 font-semibold text-base">{m.admin_version()}</h2>
            <div className="flex flex-wrap items-baseline gap-2.5">
              <span className="font-bold text-[28px] tracking-[-0.04em]">{d.version}</span>
              {d.release === null ? (
                <Chip tone="off">{m.admin_versionUnknown()}</Chip>
              ) : d.newer ? (
                <Chip tone="warn">{m.admin_newVersion({ version: d.release.version })}</Chip>
              ) : (
                <Chip tone="ok">{m.admin_upToDate()}</Chip>
              )}
            </div>
            <p className="m-0 text-[13px] text-subtle leading-relaxed">{m.admin_updateText()}</p>
            {d.release && d.newer && (
              <a href={d.release.url} target="_blank" rel="noreferrer" className="text-sm">
                {m.admin_whatsNew({ version: d.release.version })}
              </a>
            )}
          </Card>
          <Caches caches={d.caches} />
        </div>
        <Card className="gap-2.5">
          <h2 className="m-0 font-semibold text-base">{m.admin_environment()}</h2>
          {d.env.map((e) => (
            <div key={e.name} className="flex items-center justify-between gap-3 border-line border-b pb-2 text-[13px]">
              <span className="font-mono text-muted-foreground text-xs">{e.name}</span>
              <span className="truncate text-right font-mono text-subtle text-xs">
                {e.value === null ? m.admin_notSet() : e.value === 'set' ? m.admin_set() : e.value}
              </span>
            </div>
          ))}
        </Card>
      </div>
      <Visitors />
      <Log />
    </>
  );
}

const CACHE: Record<string, () => string> = {
  forecasts: m.admin_cacheForecasts,
  places: m.admin_cachePlaces,
  rankings: m.admin_cacheRankings,
  map_pieces: m.admin_cacheMapPieces,
  gauge_history: m.admin_cacheGaugeHistory,
  pins: m.admin_cachePins,
};

function Caches({ caches }: { caches: [string, number][] }) {
  const queryClient = useQueryClient();
  const clear = useMutation({
    mutationFn: () => send<void>('POST', '/admin/system/caches/clear'),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['admin', 'system'] }),
  });
  return (
    <Card className="gap-2.5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h2 className="m-0 font-semibold text-base">{m.admin_caches()}</h2>
        <Button size="sm" variant="outline" disabled={clear.isPending} onClick={() => clear.mutate()}>
          {m.admin_clearCaches()}
        </Button>
      </div>
      {caches.map(([name, n]) => (
        <div key={name} className="flex justify-between gap-3 border-line border-b pb-2 text-[13px]">
          <span>{CACHE[name]?.() ?? name}</span>
          <span className="font-mono text-subtle text-xs">{m.admin_entries({ n: formatNumber(n) })}</span>
        </div>
      ))}
    </Card>
  );
}

function Visitors() {
  const settings = useSettings();
  const s = settings.data?.settings;
  if (!s) return <Card aria-busy="true" className="h-72 animate-pulse" />;
  return (
    <Card className="gap-3">
      <h2 className="m-0 font-semibold text-base">{m.admin_visitors()}</h2>
      <div className="flex flex-col gap-1.5">
        <span className="font-medium text-[13px]">{m.admin_banner()}</span>
        <TextField
          label={m.admin_banner()}
          value={s.banner}
          placeholder={m.admin_bannerPlaceholder()}
          disabled={settings.pending}
          onSave={(v) => settings.set('banner', v)}
        />
      </div>
      <div className="grid grid-cols-[repeat(auto-fit,minmax(300px,1fr))] gap-x-5">
        <LegalText
          label={m.admin_imprint()}
          value={s.imprint}
          disabled={settings.pending}
          onSave={(v) => settings.set('imprint', v)}
        />
        <LegalText
          label={m.admin_privacy()}
          value={s.privacy}
          disabled={settings.pending}
          onSave={(v) => settings.set('privacy', v)}
        />
      </div>
      <p className="m-0 text-muted-foreground text-xs leading-relaxed">{m.admin_legalHint()}</p>
      {settings.error && <p className="m-0 text-destructive text-sm">{settings.error}</p>}
    </Card>
  );
}

function LegalText({
  label,
  value,
  disabled,
  onSave,
}: {
  label: string;
  value: string;
  disabled: boolean;
  onSave: (v: string) => void;
}) {
  const [text, setText] = useState(value);
  return (
    <form
      className="flex flex-col gap-1.5 border-line border-t pt-2.5"
      onSubmit={(e) => {
        e.preventDefault();
        onSave(text.trim());
      }}
    >
      <span className="flex items-center justify-between gap-2 font-medium text-[13px]">
        {label}
        {!value.trim() && <Chip tone="warn">{m.admin_missing()}</Chip>}
      </span>
      <textarea
        aria-label={label}
        value={text}
        rows={4}
        disabled={disabled}
        onChange={(e) => setText(e.target.value)}
        className="min-h-24 rounded-[9px] border bg-background px-3 py-2 text-[13px] outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
      />
      <Button
        type="submit"
        size="sm"
        variant="outline"
        className="self-end"
        disabled={disabled || text.trim() === value}
      >
        {m.admin_save()}
      </Button>
    </form>
  );
}

type LogKind = 'lines' | 'admin';

function Log() {
  const [kind, setKind] = useState<LogKind>('lines');
  const lines = useQuery({ ...adminLogLinesQuery, enabled: kind === 'lines' });
  const admin = useQuery({ ...adminLogQuery, enabled: kind === 'admin' });
  const zone = Intl.DateTimeFormat().resolvedOptions().timeZone;
  const at = (t: number) => `${dayMonthOf(t, zone)} ${clock(t, zone)}`;
  const error = kind === 'lines' ? lines.error : admin.error;
  return (
    <Card className="gap-3">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h2 className="m-0 font-semibold text-base">{m.admin_log()}</h2>
        <Segmented<LogKind>
          label={m.admin_log()}
          value={kind}
          options={[
            { value: 'lines', label: m.admin_logWarnings() },
            { value: 'admin', label: m.admin_logAdmin() },
          ]}
          onChange={setKind}
        />
      </div>
      {error && <Failed error={error} />}
      <div className="flex max-h-96 flex-col gap-1.5 overflow-auto rounded-xl border border-line bg-background px-3.5 py-3">
        {kind === 'lines' &&
          lines.data?.map((l, i) => (
            // biome-ignore lint/suspicious/noArrayIndexKey: a log, newest first
            <div key={i} className="flex gap-3 whitespace-nowrap font-mono text-xs">
              <span className="text-dim">{at(l.at)}</span>
              <span className={l.level === 'ERROR' ? 'text-destructive' : 'text-warning-foreground'}>{l.level}</span>
              <span className="text-subtle">{l.message}</span>
            </div>
          ))}
        {kind === 'admin' &&
          admin.data?.map((l, i) => (
            // biome-ignore lint/suspicious/noArrayIndexKey: a log, newest first
            <div key={i} className="flex gap-3 whitespace-nowrap font-mono text-xs">
              <span className="text-dim">{at(l.at)}</span>
              <span className="text-subtle">{l.who ?? m.admin_logCli()}</span>
              <span>{l.action}</span>
              <span className="text-muted-foreground">{l.target ?? ''}</span>
            </div>
          ))}
        {((kind === 'lines' && lines.data?.length === 0) || (kind === 'admin' && admin.data?.length === 0)) && (
          <span className="text-muted-foreground text-sm">{m.admin_logEmpty()}</span>
        )}
      </div>
    </Card>
  );
}
