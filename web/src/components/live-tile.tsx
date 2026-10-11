import { useQuery } from '@tanstack/react-query';
import { ChevronDown, Code } from 'lucide-react';
import { useId, useMemo, useState } from 'react';
import type { Observation } from '@/bindings/Observation';
import type { Station } from '@/bindings/Station';
import { liveQuery } from '@/lib/api';
import { ago, clock, compass } from '@/lib/format';
import { decodeMetar } from '@/lib/metar';
import { type Context, describe, summary } from '@/lib/metar-text';
import { latest, STALE_S } from '@/lib/observations';
import { useUnit } from '@/lib/prefs';
import { formatWind, UNIT_LABEL } from '@/lib/units';
import { useNow } from '@/lib/use-now';
import { cn } from '@/lib/utils';
import { m } from '@/paraglide/messages';
import { Card, CardHeader, CardMeta, CardTitle } from '@/ui/card';
import { WindArrow } from '@/ui/wind-arrow';

/** What the station measures now: wind, direction, gusts, and for METARs the report in words. */
export function LiveTile({ station, timeZone }: { station: Station; timeZone: string }) {
  const q = useQuery(liveQuery(station.slug));
  const now = useNow();
  const obs = q.data ? latest(q.data.observations) : null;
  const fresh = obs !== null && now - obs.at < STALE_S;
  const source = station.source === 'metar' ? 'METAR' : 'DWD';

  return (
    <Card aria-labelledby="live-title" aria-busy={q.isPending}>
      <CardHeader className="flex-wrap">
        <CardTitle id="live-title" className="whitespace-nowrap">
          <span
            aria-hidden="true"
            className={cn('size-2 rounded-full', fresh ? 'bg-success ring-4 ring-success-ring' : 'bg-dim')}
          />
          {m.live_title()}
        </CardTitle>
        {obs && (
          <CardMeta>
            {source} · {m.live_clock({ time: clock(obs.at, timeZone) })} · {ago(obs.at, now)}
            {!fresh && ` · ${m.live_old()}`}
          </CardMeta>
        )}
      </CardHeader>
      {q.isPending ? (
        <div className="h-[76px] animate-pulse rounded-lg bg-muted" />
      ) : obs ? (
        <Measured obs={obs} station={station} timeZone={timeZone} />
      ) : (
        <p className="m-0 text-muted-foreground text-sm">{m.live_none()}</p>
      )}
    </Card>
  );
}

function Measured({ obs, station, timeZone }: { obs: Observation; station: Station; timeZone: string }) {
  const [unit] = useUnit();
  const wind = obs.wind ?? 0;
  return (
    <>
      <div className="flex flex-wrap items-end gap-x-5 gap-y-2">
        <div className="flex items-baseline gap-1.5">
          <span className="font-mono font-semibold text-[56px] leading-[0.9] tracking-[-0.06em] sm:text-[76px]">
            {formatWind(wind, unit)}
          </span>
          <span className="text-muted-foreground">{UNIT_LABEL[unit]}</span>
        </div>
        <div className="flex flex-col gap-1.5 pb-1.5 text-sm">
          <span className="flex items-center gap-1.5 font-medium">
            {obs.dir !== null ? (
              <>
                <WindArrow deg={obs.dir} />
                {compass(obs.dir)} · {Math.round(obs.dir)}°
              </>
            ) : wind === 0 ? (
              m.live_calm()
            ) : (
              m.live_variable()
            )}
          </span>
          {obs.gust !== null && (
            <span className="text-muted-foreground">
              {m.live_gusts()} <strong className="font-mono text-foreground">{formatWind(obs.gust, unit)}</strong>
            </span>
          )}
        </div>
      </div>
      {obs.raw && station.source === 'metar' && <Metar raw={obs.raw} ctx={{ unit, at: obs.at, timeZone }} />}
    </>
  );
}

function Metar({ raw, ctx }: { raw: string; ctx: Context }) {
  const [open, setOpen] = useState(false);
  const id = useId();
  const groups = useMemo(() => decodeMetar(raw), [raw]);
  return (
    <>
      <ul className="m-0 flex list-none flex-wrap gap-1.5 p-0">
        {summary(groups).map((g) => (
          <li
            key={g.token}
            className="rounded-full border bg-secondary px-2.5 py-1 text-subtle text-xs first-letter:uppercase"
          >
            {describe(g, ctx)}
          </li>
        ))}
      </ul>
      <button
        type="button"
        aria-expanded={open}
        aria-controls={id}
        onClick={() => setOpen(!open)}
        className="flex min-h-11 w-full cursor-pointer items-center justify-between rounded-[10px] border bg-background px-3 font-medium text-[13px] outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
      >
        <span className="flex items-center gap-2">
          <Code className="size-4 text-muted-foreground" aria-hidden="true" />
          {m.live_raw()}
        </span>
        <ChevronDown
          className={cn('size-4 text-muted-foreground transition-transform', open && 'rotate-180')}
          aria-hidden="true"
        />
      </button>
      {open && (
        <div id={id} className="flex flex-col gap-2.5 rounded-[10px] border bg-background p-3">
          <code className="break-words font-mono text-[12.5px] leading-normal">{raw}</code>
          <dl className="m-0 grid grid-cols-[max-content_1fr] gap-x-3 gap-y-1.5 text-xs leading-snug">
            {groups.map((g, i) => (
              // biome-ignore lint/suspicious/noArrayIndexKey: tokens can repeat; the list is fixed per report
              <div key={i} className="contents">
                <dt className="break-all font-mono text-muted-foreground">{g.token}</dt>
                <dd className="m-0 text-subtle first-letter:uppercase">{describe(g, ctx)}</dd>
              </div>
            ))}
          </dl>
        </div>
      )}
    </>
  );
}
