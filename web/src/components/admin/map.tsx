/** The admin page's map: the basemap with its renewal (automatic, ask, never; now, later,
 * cancel), the relief, the depth, the detail spots and the wind map's model fields. */
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Download } from 'lucide-react';
import { useState } from 'react';
import type { BasemapMode } from '@/bindings/BasemapMode';
import type { BasemapState } from '@/bindings/BasemapState';
import type { Due } from '@/bindings/Due';
import type { MapRun } from '@/bindings/MapRun';
import type { SpotInfo } from '@/bindings/SpotInfo';
import type { SpotReason } from '@/bindings/SpotReason';
import type { Work } from '@/bindings/Work';
import { Failed } from '@/components/page';
import { buildDate, bytes, date, parseRegion, regionText, remaining, span } from '@/lib/admin';
import { adminMapQuery } from '@/lib/admin-api';
import { isApiError, send } from '@/lib/api';
import { ago, coordinates, formatNumber, runHour } from '@/lib/format';
import { describeError } from '@/lib/i18n';
import { cn } from '@/lib/utils';
import { m } from '@/paraglide/messages';
import { Button } from '@/ui/button';
import { Card } from '@/ui/card';
import { Segmented } from '@/ui/segmented';
import { Chip, ROW, Row, TabHead, Table } from './parts';
import { FIELD, NumberField, SettingRow, Toggle, useSettings } from './setting';

const DUE: Record<Due, () => string> = {
  missing: m.admin_dueMissing,
  old: m.admin_dueOld,
  changed: m.admin_dueChanged,
  legacy: m.admin_dueLegacy,
};

export function MapTab() {
  const map = useQuery(adminMapQuery);
  if (map.isError) {
    if (isApiError(map.error) && map.error.status === 404)
      return (
        <>
          <TabHead title={m.admin_tabMap()} />
          <p className="m-0 text-muted-foreground">{m.admin_noMap()}</p>
        </>
      );
    return <Failed error={map.error} />;
  }
  if (!map.data) return <div aria-busy="true" className="h-96 animate-pulse rounded-xl bg-muted" />;
  const b = map.data.basemap;
  return (
    <>
      <TabHead title={m.admin_tabMap()} lead={m.admin_mapLead()} />
      <BasemapCard state={b} />
      <div className="grid grid-cols-[repeat(auto-fit,minmax(340px,1fr))] items-start gap-5">
        <TerrainCard state={b} />
        <DepthCard />
      </div>
      <SpotsCard spots={b.spots} />
      <ModelsCard runs={map.data.runs} />
    </>
  );
}

function useAction() {
  const queryClient = useQueryClient();
  const [message, setMessage] = useState<string | null>(null);
  const act = useMutation({
    mutationFn: ({ method, path, body }: { method: 'POST' | 'PUT' | 'DELETE'; path: string; body?: unknown }) =>
      send<unknown>(method, `/admin/map${path}`, body),
    onSuccess: async (data) => {
      setMessage(typeof data === 'number' ? m.admin_leftoversDone({ count: data }) : null);
      await queryClient.invalidateQueries({ queryKey: ['admin'] });
    },
    onError: (e) => setMessage(describeError(e)),
  });
  return {
    run: (method: 'POST' | 'PUT' | 'DELETE', path: string, body?: unknown) => act.mutate({ method, path, body }),
    pending: act.isPending,
    message,
  };
}

function BasemapCard({ state }: { state: BasemapState }) {
  const settings = useSettings();
  const action = useAction();
  const s = settings.data?.settings;
  const now = Date.now() / 1000;
  const work = state.work.filter((w) => w.what === 'basemap' || w.what === 'terrain');
  const due = state.due ?? state.terrainDue;
  const postponed = s && s.basemapPostponedUntil > now ? s.basemapPostponedUntil : null;
  const mode = s?.basemapMode ?? 'auto';
  return (
    <Card className="gap-4">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div className="flex flex-col gap-1">
          <h2 className="m-0 font-semibold text-base">{m.admin_basemap()}</h2>
          {state.file && (
            <span className="font-mono text-muted-foreground text-xs">
              {state.file.worldZoom !== null && m.admin_depthWorld({ zoom: state.file.worldZoom })}
              {state.file.region && ` · ${m.admin_depthRegion({ zoom: state.file.regionZoom })}`}
            </span>
          )}
        </div>
        <Segmented<BasemapMode>
          label={m.admin_renewMode()}
          value={mode}
          options={[
            { value: 'auto', label: m.admin_modeAuto(), disabled: !!settings.locked('basemapMode') },
            { value: 'ask', label: m.admin_modeAsk(), disabled: !!settings.locked('basemapMode') },
            { value: 'off', label: m.admin_modeOff(), disabled: !!settings.locked('basemapMode') },
          ]}
          onChange={(v) => settings.set('basemapMode', v)}
        />
      </div>
      {state.operator && <p className="m-0 text-muted-foreground text-sm">{m.admin_operatorFile()}</p>}
      {!state.file && !state.operator && <p className="m-0 text-muted-foreground text-sm">{m.admin_noBasemap()}</p>}
      {state.file && (
        <div className="grid grid-cols-[repeat(auto-fit,minmax(160px,1fr))] gap-2.5">
          <Fact label={m.admin_stand()} value={m.admin_build({ date: buildDate(state.file.build) })}>
            {m.admin_age({ days: Math.floor((now - state.file.madeAt) / 86_400) })}
          </Fact>
          <Fact label={m.admin_renewAt()} value={date(state.file.renewAt)} />
        </div>
      )}
      {work.map((w) => (
        <WorkBar key={w.what} work={w} onCancel={() => action.run('POST', '/cancel')} />
      ))}
      {work.length === 0 && due && mode !== 'off' && !postponed && !state.approved && (
        <div className="flex flex-wrap items-center gap-3.5 rounded-xl border border-warning-border bg-warning-surface px-4 py-3.5 text-warning-foreground">
          <Download aria-hidden="true" className="size-[18px] shrink-0" />
          <div className="flex flex-[1_1_320px] flex-col gap-0.5 text-[13px] leading-snug">
            <strong className="font-semibold">{m.admin_pendingTitle({ reason: DUE[due]() })}</strong>
            <span>
              {state.newestBuild
                ? m.admin_pendingNewest({ build: buildDate(state.newestBuild) })
                : m.admin_pendingNoBuild()}
            </span>
          </div>
          <Button
            size="sm"
            variant="outline"
            className="border-warning-border bg-transparent text-warning-foreground"
            disabled={action.pending}
            onClick={() => action.run('POST', '/postpone', { days: 30 })}
          >
            {m.admin_later()}
          </Button>
          <Button size="sm" disabled={action.pending} onClick={() => action.run('POST', '/renew')}>
            {m.admin_renewNow()}
          </Button>
        </div>
      )}
      {state.approved && work.length === 0 && <p className="m-0 text-sm text-subtle">{m.admin_approved()}</p>}
      {postponed && (
        <div className="flex flex-wrap items-center gap-3.5 rounded-xl border bg-background px-4 py-3 text-[13px] text-subtle">
          <span className="flex-[1_1_300px]">{m.admin_postponed({ date: date(postponed) })}</span>
          <Button size="sm" variant="outline" onClick={() => settings.set('basemapPostponedUntil', 0)}>
            {m.admin_undo()}
          </Button>
        </div>
      )}
      {(action.message || settings.error) && (
        <p role="status" className="m-0 text-muted-foreground text-sm">
          {action.message ?? settings.error}
        </p>
      )}
    </Card>
  );
}

function Fact({ label, value, children }: { label: string; value: string; children?: string }) {
  return (
    <div className="flex flex-col gap-1 rounded-xl border border-line px-3.5 py-3">
      <span className="text-muted-foreground text-xs">{label}</span>
      <span className="font-semibold text-[15px]">{value}</span>
      {children && <span className="text-warning-foreground text-xs">{children}</span>}
    </div>
  );
}

const WORK_NAME: Record<string, () => string> = {
  basemap: m.admin_workBasemap,
  terrain: m.admin_workTerrain,
  spot: m.admin_workSpot,
};

function WorkBar({ work, onCancel }: { work: Work; onCancel: () => void }) {
  const what = WORK_NAME[work.what]?.() ?? work.what;
  const left = remaining(work.done, work.total, work.rate);
  const share = work.total > 0 ? Math.min(100, (work.done / work.total) * 100) : 0;
  const text =
    work.unit === 'bytes'
      ? m.admin_workBytes({
          what,
          done: bytes(work.done),
          total: bytes(work.total),
          rate: bytes(work.rate),
          left: left === null ? '–' : span(left),
        })
      : m.admin_workTiles({
          what,
          done: formatNumber(work.done),
          total: formatNumber(work.total),
          rate: formatNumber(work.rate),
          left: left === null ? '–' : span(left),
        });
  return (
    <div className="flex flex-col gap-2.5 rounded-xl border bg-background px-4 py-3.5">
      <span className="font-mono text-[13px] text-subtle">{text}</span>
      <div
        role="progressbar"
        aria-valuenow={Math.round(share)}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-label={what}
        className="h-2 overflow-hidden rounded-full bg-line"
      >
        <div className="h-full rounded-full bg-foreground" style={{ width: `${share}%` }} />
      </div>
      <div className="flex justify-end">
        <Button size="sm" variant="outline" className="border-destructive/40 text-destructive" onClick={onCancel}>
          {m.admin_cancelWork()}
        </Button>
      </div>
    </div>
  );
}

function TerrainCard({ state }: { state: BasemapState }) {
  return (
    <Card className="gap-3">
      <div className="flex justify-between gap-3">
        <h2 className="m-0 font-semibold text-base">{m.admin_terrain()}</h2>
        {state.terrainDue && <Chip tone="warn">{DUE[state.terrainDue]()}</Chip>}
      </div>
      {state.terrainOperator && <p className="m-0 text-muted-foreground text-sm">{m.admin_operatorFile()}</p>}
      {state.terrain ? (
        <>
          <span className="font-mono text-muted-foreground text-xs">
            {m.admin_terrainZoom({ zoom: state.terrain.zoom })}
          </span>
          <Row label={m.admin_stand()}>{date(state.terrain.madeAt)}</Row>
          <Row label={m.admin_renewAt()}>{date(state.terrain.renewAt)}</Row>
        </>
      ) : (
        !state.terrainOperator && <p className="m-0 text-muted-foreground text-sm">{m.admin_noTerrain()}</p>
      )}
    </Card>
  );
}

function DepthCard() {
  const settings = useSettings();
  const s = settings.data?.settings;
  if (!s) return <Card aria-busy="true" className="h-72 animate-pulse" />;
  const busy = settings.pending;
  return (
    <Card className="gap-3">
      <h2 className="m-0 font-semibold text-base">{m.admin_depth()}</h2>
      <SettingRow
        label={m.admin_worldZoom()}
        hint={m.admin_worldZoomHint()}
        lockedBy={settings.locked('basemapWorldZoom')}
      >
        <NumberField
          label={m.admin_worldZoom()}
          value={s.basemapWorldZoom}
          min={4}
          max={12}
          disabled={busy || !!settings.locked('basemapWorldZoom')}
          onSave={(n) => settings.set('basemapWorldZoom', n)}
        />
      </SettingRow>
      <SettingRow label={m.admin_region()} hint={m.admin_regionHint()} lockedBy={settings.locked('basemapRegion')}>
        <RegionField
          value={s.basemapRegion}
          disabled={busy || !!settings.locked('basemapRegion')}
          onSave={(r) => settings.set('basemapRegion', r)}
        />
      </SettingRow>
      <SettingRow
        label={m.admin_regionZoom()}
        hint={m.admin_regionZoomHint()}
        lockedBy={settings.locked('basemapZoom')}
      >
        <NumberField
          label={m.admin_regionZoom()}
          value={s.basemapZoom}
          min={6}
          max={15}
          disabled={busy || !!settings.locked('basemapZoom')}
          onSave={(n) => settings.set('basemapZoom', n)}
        />
      </SettingRow>
      <SettingRow
        label={m.admin_terrainZoomLabel()}
        hint={m.admin_terrainZoomHint()}
        lockedBy={settings.locked('terrainZoom')}
      >
        <NumberField
          label={m.admin_terrainZoomLabel()}
          value={s.terrainZoom}
          min={0}
          max={12}
          disabled={busy || !!settings.locked('terrainZoom')}
          onSave={(n) => settings.set('terrainZoom', n)}
        />
      </SettingRow>
      <SettingRow label={m.admin_spotsMax()} hint={m.admin_spotsMaxHint()} lockedBy={settings.locked('basemapSpots')}>
        <NumberField
          label={m.admin_spotsMax()}
          value={s.basemapSpots}
          min={0}
          max={10_000}
          disabled={busy || !!settings.locked('basemapSpots')}
          onSave={(n) => settings.set('basemapSpots', n)}
        />
      </SettingRow>
      <p className="m-0 text-muted-foreground text-xs">{m.admin_depthNote()}</p>
      {settings.error && <p className="m-0 text-destructive text-sm">{settings.error}</p>}
    </Card>
  );
}

function RegionField({
  value,
  disabled,
  onSave,
}: {
  value: [number, number, number, number] | null;
  disabled: boolean;
  onSave: (r: [number, number, number, number] | null) => void;
}) {
  const [text, setText] = useState(regionText(value));
  const save = () => {
    const r = parseRegion(text);
    if (r === undefined || regionText(r) === regionText(value)) setText(regionText(value));
    else onSave(r);
  };
  return (
    <input
      aria-label={m.admin_region()}
      value={text}
      disabled={disabled}
      onChange={(e) => setText(e.target.value)}
      onBlur={save}
      onKeyDown={(e) => e.key === 'Enter' && save()}
      className={cn(FIELD, 'w-40')}
    />
  );
}

const REASON: Record<SpotReason, () => string> = {
  pinned: m.admin_reasonPinned,
  place: m.admin_reasonPlace,
  tapped: m.admin_reasonTapped,
  unwanted: m.admin_reasonUnwanted,
};

function SpotsCard({ spots }: { spots: SpotInfo[] }) {
  const action = useAction();
  const total = spots.reduce((a, s) => a + s.bytes, 0);
  return (
    <Card className="gap-3.5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex flex-col gap-1">
          <h2 className="m-0 font-semibold text-base">{m.admin_spots()}</h2>
          <span className="text-muted-foreground text-xs">
            {m.admin_spotsLead({ count: spots.length, size: bytes(total) })}
          </span>
        </div>
        <Button size="sm" variant="outline" disabled={action.pending} onClick={() => action.run('POST', '/leftovers')}>
          {m.admin_leftovers()}
        </Button>
      </div>
      {action.message && (
        <p role="status" className="m-0 text-muted-foreground text-sm">
          {action.message}
        </p>
      )}
      {spots.length === 0 ? (
        <p className="m-0 text-muted-foreground text-sm">{m.admin_noSpots()}</p>
      ) : (
        <Table
          cols="2fr 1.4fr 0.8fr 1fr 1.6fr"
          head={[
            m.admin_colSpot(),
            m.admin_colWhy(),
            m.admin_colSize(),
            m.admin_colWanted(),
            <span key="a" className="block text-right">
              {m.admin_colAction()}
            </span>,
          ]}
        >
          {spots.map((s) => (
            <div key={`${s.lat}_${s.lon}`} className={ROW}>
              <span className="font-mono text-xs">{coordinates(s.lat, s.lon)}</span>
              <span className="text-subtle">{REASON[s.reason]()}</span>
              <span className="font-mono text-muted-foreground text-xs">{bytes(s.bytes)}</span>
              <span className="font-mono text-subtle text-xs">{ago(s.wantedAt)}</span>
              <span className="flex justify-end gap-1.5">
                <Button
                  size="sm"
                  variant="outline"
                  disabled={action.pending}
                  onClick={() =>
                    action.run(s.reason === 'pinned' ? 'DELETE' : 'PUT', '/spots/pinned', { lat: s.lat, lon: s.lon })
                  }
                >
                  {s.reason === 'pinned' ? m.admin_unpin() : m.admin_pin()}
                </Button>
                <Button
                  size="sm"
                  variant="outline"
                  className="text-destructive"
                  disabled={action.pending}
                  onClick={() => action.run('DELETE', `/spots?lat=${s.lat}&lon=${s.lon}`)}
                >
                  {m.admin_remove()}
                </Button>
              </span>
            </div>
          ))}
        </Table>
      )}
    </Card>
  );
}

const MODEL_NAME: Record<string, string> = {
  icon_d2: 'ICON-D2',
  ecmwf_ifs025: 'ECMWF IFS',
  icon_global: 'ICON global',
};

function ModelsCard({ runs }: { runs: [string, MapRun[]][] }) {
  const settings = useSettings();
  const s = settings.data?.settings;
  return (
    <Card className="gap-3.5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h2 className="m-0 font-semibold text-base">{m.admin_mapModels()}</h2>
        <span className="text-muted-foreground text-xs">{m.admin_mapModelsHint()}</span>
      </div>
      <div className="grid grid-cols-[repeat(auto-fit,minmax(230px,1fr))] gap-2.5">
        {runs.map(([model, list]) => {
          const newest = list[0];
          return (
            <div key={model} className="flex flex-col gap-2 rounded-xl border border-line bg-background p-3.5">
              <div className="flex items-center justify-between gap-2">
                <span className="font-semibold text-[15px]">{MODEL_NAME[model] ?? model}</span>
                {model === 'icon_global' && s && (
                  <Toggle
                    label={m.admin_iconGlobal()}
                    on={s.iconGlobal}
                    disabled={settings.pending || !!settings.locked('iconGlobal')}
                    onChange={(on) => settings.set('iconGlobal', on)}
                  />
                )}
              </div>
              <span className="font-mono text-subtle text-xs">
                {newest
                  ? `${m.admin_run({ run: runHour(newest.runAt) })} · ${m.admin_hours({ count: newest.steps.length })}`
                  : m.admin_noRuns()}
              </span>
              {model === 'icon_global' && (
                <span className="text-muted-foreground text-xs">{m.admin_iconGlobalHint()}</span>
              )}
            </div>
          );
        })}
      </div>
    </Card>
  );
}
