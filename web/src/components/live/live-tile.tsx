import { useQuery } from '@tanstack/react-query';
import { ChevronDown, Code, Plane } from 'lucide-react';
import { type ReactNode, useId, useMemo, useState } from 'react';
import type { Observation } from '@/bindings/Observation';
import type { Sector } from '@/bindings/Sector';
import type { Station } from '@/bindings/Station';
import type { Taf } from '@/bindings/Taf';
import { favoritesQuery, liveQuery, meQuery } from '@/lib/api';
import { variation } from '@/lib/compass-rose';
import { findFavorite, inSector } from '@/lib/favorites';
import { ago, clock, compass } from '@/lib/format';
import { decodeMetar } from '@/lib/metar';
import { type Context, describe, summary } from '@/lib/metar-text';
import { latest, STALE_S } from '@/lib/observations';
import { useUnit } from '@/lib/prefs';
import { groupWhat, groupWhen, prevailingBefore } from '@/lib/taf-text';
import { formatWind, UNIT_LABEL } from '@/lib/units';
import { useNow } from '@/lib/use-now';
import { cn } from '@/lib/utils';
import { m } from '@/paraglide/messages';
import { Card, CardHeader, CardMeta, CardTitle } from '@/ui/card';
import { WindArrow } from '@/ui/wind-arrow';
import { WindCompass } from '@/ui/wind-compass';

/** What the station measures now: wind, direction, gusts, and for METARs the report in words. */
export function LiveTile({ station, timeZone }: { station: Station; timeZone: string }) {
  const q = useQuery(liveQuery(station.slug));
  const me = useQuery(meQuery);
  const favorites = useQuery({ ...favoritesQuery, enabled: Boolean(me.data) });
  // The directions that work, where the station is a saved spot
  const sector = (favorites.data && findFavorite(favorites.data, { slug: station.slug })?.sector) ?? null;
  const now = useNow();
  const obs = q.data ? latest(q.data.observations) : null;
  const taf = q.data?.taf ? <TafBlock taf={q.data.taf} timeZone={timeZone} now={now} /> : null;
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
        <Measured obs={obs} station={station} timeZone={timeZone} taf={taf} sector={sector} />
      ) : (
        <>
          <p className="m-0 text-muted-foreground text-sm">{m.live_none()}</p>
          {taf}
        </>
      )}
    </Card>
  );
}

/** The airport forecast, folded like the raw METAR so the forecast below comes sooner: group by
 * group in words, with the TAF as sent, once opened. */
function TafBlock({ taf, timeZone, now }: { taf: Taf; timeZone: string; now: number }) {
  const [unit] = useUnit();
  const [open, setOpen] = useState(false);
  const id = useId();
  const ctx = { unit, at: taf.issuedAt, timeZone };
  const prev = prevailingBefore(taf.groups);
  return (
    <section aria-labelledby={`${id}-title`} className="flex flex-col gap-2.5">
      <button
        type="button"
        aria-expanded={open}
        aria-controls={id}
        onClick={() => setOpen(!open)}
        className="flex min-h-11 w-full cursor-pointer items-center justify-between gap-2 rounded-[10px] border bg-background px-3 text-left font-medium text-[13px] outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
      >
        <span className="flex min-w-0 items-center gap-2">
          <Plane className="size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
          <span id={`${id}-title`}>{m.taf_title()}</span>
        </span>
        <span className="flex shrink-0 items-center gap-2">
          <span className="font-mono text-[11px] text-muted-foreground">{ago(taf.issuedAt, now)}</span>
          <ChevronDown
            className={cn('size-4 text-muted-foreground transition-transform', open && 'rotate-180')}
            aria-hidden="true"
          />
        </span>
      </button>
      {open && (
        <div id={id} className="flex flex-col gap-2.5 rounded-[10px] border bg-background p-3">
          <span className="font-mono text-[11px] text-muted-foreground">
            {m.live_clock({ time: clock(taf.issuedAt, timeZone) })} · {ago(taf.issuedAt, now)}
          </span>
          <dl className="m-0 grid grid-cols-[max-content_1fr] gap-x-2.5 gap-y-1.5 text-[13px] text-subtle leading-snug">
            {taf.groups.map((g, i) => (
              <div key={`${g.change}-${g.from}-${g.to}-${g.probability}`} className="contents">
                <dt className="pt-px font-mono text-[11.5px] text-muted-foreground">{groupWhen(g, now, timeZone)}</dt>
                <dd className="m-0 min-w-0 first-letter:uppercase">{groupWhat(g, ctx, prev[i])}</dd>
              </div>
            ))}
          </dl>
          <code className="break-words font-mono text-[11px] text-dim leading-normal">{taf.raw}</code>
        </div>
      )}
    </section>
  );
}

function Measured({
  obs,
  station,
  timeZone,
  taf,
  sector,
}: {
  obs: Observation;
  station: Station;
  timeZone: string;
  /** The folded TAF, before the raw METAR */
  taf?: ReactNode;
  sector: Sector | null;
}) {
  const [unit] = useUnit();
  const wind = obs.wind ?? 0;
  const fits = sector && obs.dir !== null ? inSector(sector, obs.dir) : null;
  return (
    <>
      <div className="flex items-center gap-3">
        <div className="flex min-w-0 flex-1 flex-wrap items-end gap-x-5 gap-y-2">
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
            {fits !== null && (
              <span
                className={cn(
                  'flex items-center gap-1.5 text-xs',
                  fits ? 'text-success-foreground' : 'text-muted-foreground',
                )}
              >
                <span aria-hidden="true" className={cn('size-1.5 rounded-full', fits ? 'bg-success' : 'bg-dim')} />
                {fits ? m.live_fits() : m.live_fitsNot()}
              </span>
            )}
          </div>
        </div>
        <WindCompass
          dir={obs.dir}
          calm={wind === 0}
          range={station.source === 'metar' ? variation(obs.raw) : null}
          sector={sector}
          className="size-[88px] sm:size-[116px]"
        />
      </div>
      {obs.raw && station.source === 'metar' ? (
        <Metar raw={obs.raw} ctx={{ unit, at: obs.at, timeZone }} before={taf} />
      ) : (
        taf
      )}
    </>
  );
}

function Metar({ raw, ctx, before }: { raw: string; ctx: Context; before?: ReactNode }) {
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
      {before}
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
