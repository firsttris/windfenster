/** The admin page's storage: the data folder by part, rows per table, backup, compacting and
 * removing what ran out. */
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import type { ReactNode } from 'react';
import type { DiskUsage } from '@/bindings/DiskUsage';
import { Failed } from '@/components/page';
import { bytes, diskUsed } from '@/lib/admin';
import { adminStorageQuery } from '@/lib/admin-api';
import { send } from '@/lib/api';
import { formatNumber } from '@/lib/format';
import { describeError } from '@/lib/i18n';
import { m } from '@/paraglide/messages';
import { Button } from '@/ui/button';
import { Card } from '@/ui/card';
import { TabHead } from './parts';

export function StorageTab() {
  const q = useQuery(adminStorageQuery);
  if (q.isError) return <Failed error={q.error} />;
  if (!q.data) return <div aria-busy="true" className="h-96 animate-pulse rounded-xl bg-muted" />;
  const d = q.data;
  return (
    <>
      <TabHead
        title={m.admin_tabStorage()}
        lead={d.disk && m.admin_storageLead({ used: bytes(diskUsed(d.disk)), free: bytes(d.disk.free) })}
      />
      {d.disk && <UsageCard disk={d.disk} />}
      <div className="grid grid-cols-[repeat(auto-fit,minmax(340px,1fr))] items-start gap-5">
        <Card className="gap-2.5">
          <h2 className="m-0 font-semibold text-base">{m.admin_tables()}</h2>
          {d.tables.map((t) => (
            <div key={t.table} className="flex justify-between gap-3 border-line border-b pb-2 text-[13px]">
              <span className="font-mono text-xs">{t.table}</span>
              <span className="font-mono text-subtle text-xs">{formatNumber(t.rows)}</span>
            </div>
          ))}
        </Card>
        <CareCard sessions={d.stale.sessions} links={d.stale.links} inactive={d.stale.inactiveAccounts} />
      </div>
    </>
  );
}

const PARTS: {
  key: string;
  color: string;
  name: () => string;
  note: (d: DiskUsage) => string;
  size: (d: DiskUsage) => number;
}[] = [
  {
    key: 'models',
    color: 'bg-chart-1',
    name: m.admin_partModels,
    note: (d) =>
      d.models
        .filter(([, b]) => b > 0)
        .map(([id, b]) => `${id} ${bytes(b)}`)
        .join(' · '),
    size: (d) => d.models.reduce((a, [, b]) => a + b, 0),
  },
  {
    key: 'terrain',
    color: 'bg-chart-4',
    name: m.admin_partTerrain,
    note: () => 'terrain.pmtiles',
    size: (d) => d.terrain,
  },
  {
    key: 'database',
    color: 'bg-foreground',
    name: m.admin_partDatabase,
    note: () => m.admin_partDatabaseNote(),
    size: (d) => d.database,
  },
  {
    key: 'spots',
    color: 'bg-chart-mix',
    name: m.admin_partSpots,
    note: (d) => m.admin_partSpotsNote({ count: d.spotFiles }),
    size: (d) => d.spots,
  },
  {
    key: 'basemap',
    color: 'bg-chart-3',
    name: m.admin_partBasemap,
    note: () => 'basemap.pmtiles',
    size: (d) => d.basemap,
  },
  {
    key: 'partial',
    color: 'bg-destructive',
    name: m.admin_partPartial,
    note: () => m.admin_partPartialNote(),
    size: (d) => d.partial,
  },
  { key: 'other', color: 'bg-dim', name: m.admin_partOther, note: () => m.admin_partOtherNote(), size: (d) => d.other },
];

function UsageCard({ disk }: { disk: DiskUsage }) {
  const used = Math.max(1, diskUsed(disk));
  return (
    <Card className="gap-3.5">
      <h2 className="m-0 font-semibold text-base">{m.admin_usage()}</h2>
      <div role="img" aria-label={m.admin_usage()} className="flex h-4 gap-0.5 overflow-hidden rounded-lg bg-line">
        {PARTS.map((p) => (
          <div key={p.key} className={`h-full ${p.color}`} style={{ width: `${(p.size(disk) / used) * 100}%` }} />
        ))}
      </div>
      <div className="grid grid-cols-[repeat(auto-fill,minmax(210px,1fr))] gap-2.5">
        {PARTS.map((p) => (
          <div key={p.key} className="flex items-start gap-2.5 text-[13px]">
            <span className={`mt-1 size-2.5 shrink-0 rounded-[3px] ${p.color}`} />
            <span className="flex flex-col">
              <span className="font-medium">
                {p.name()} · <span className="font-mono">{bytes(p.size(disk))}</span>
              </span>
              <span className="text-muted-foreground text-xs">{p.note(disk)}</span>
            </span>
          </div>
        ))}
      </div>
    </Card>
  );
}

function Item({ title, text, children }: { title: string; text: string; children: ReactNode }) {
  return (
    <div className="flex flex-wrap items-center justify-between gap-3 border-line border-b pb-2.5">
      <span className="flex flex-[1_1_220px] flex-col gap-0.5">
        <span className="font-medium text-[13px]">{title}</span>
        <span className="text-muted-foreground text-xs leading-snug">{text}</span>
      </span>
      <span className="flex gap-1.5">{children}</span>
    </div>
  );
}

function CareCard({ sessions, links, inactive }: { sessions: number; links: number; inactive: number }) {
  const queryClient = useQueryClient();
  const act = useMutation({
    mutationFn: (path: string) => send<unknown>('POST', path),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['admin'] }),
  });
  return (
    <Card className="gap-3">
      <h2 className="m-0 font-semibold text-base">{m.admin_care()}</h2>
      <Item title={m.admin_backup()} text={m.admin_backupText()}>
        <Button size="sm" asChild>
          <a href="/api/admin/storage/backup" download>
            {m.admin_backupDb()}
          </a>
        </Button>
        <Button size="sm" variant="outline" asChild>
          <a href="/api/admin/storage/push-key" download>
            {m.admin_backupKey()}
          </a>
        </Button>
      </Item>
      <Item title={m.admin_vacuum()} text={m.admin_vacuumText()}>
        <Button
          size="sm"
          variant="outline"
          disabled={act.isPending}
          onClick={() => act.mutate('/admin/storage/vacuum')}
        >
          {m.admin_vacuumGo()}
        </Button>
      </Item>
      <Item title={m.admin_prune()} text={m.admin_pruneText({ sessions, links })}>
        <Button
          size="sm"
          variant="outline"
          disabled={act.isPending || sessions + links === 0}
          onClick={() => act.mutate('/admin/storage/prune')}
        >
          {m.admin_pruneGo()}
        </Button>
      </Item>
      <Item title={m.admin_inactive()} text={m.admin_inactiveText({ count: inactive })}>
        <Button size="sm" variant="outline" asChild>
          <a href="/admin?tab=users">{m.admin_look()}</a>
        </Button>
      </Item>
      {act.isError && <p className="m-0 text-destructive text-sm">{describeError(act.error)}</p>}
    </Card>
  );
}
