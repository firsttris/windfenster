import { useQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { useMemo, useState } from 'react';
import { basemapFor } from '@/components/map/basemap';
import { Legend } from '@/components/map/legend';
import { MapCanvas, type Pick } from '@/components/map/map-canvas';
import { SpotCard } from '@/components/map/spot-card';
import { frameTime, Timeline } from '@/components/map/timeline';
import { usePlayback, useWindPair } from '@/components/map/use-wind-pair';
import { mapIndexQuery, mapStationsQuery, modelsQuery } from '@/lib/api';
import { formatNumber, runHour } from '@/lib/format';
import { useUnit } from '@/lib/prefs';
import { useMedia } from '@/lib/use-media';
import { useNow } from '@/lib/use-now';
import { useTitle } from '@/lib/use-title';
import { cn } from '@/lib/utils';
import { chooseLayers, hoursAt, positionAt, startTime } from '@/lib/wind-map';
import { m } from '@/paraglide/messages';

export const Route = createFileRoute('/map')({
  loader: ({ context }) => {
    void context.queryClient.prefetchQuery(mapIndexQuery);
    void context.queryClient.prefetchQuery(mapStationsQuery);
  },
  component: MapPage,
});

function MapPage() {
  useTitle(m.title_map());
  const index = useQuery(mapIndexQuery);
  const pins = useQuery(mapStationsQuery);
  const models = useQuery(modelsQuery);
  const [unit] = useUnit();
  const now = useNow();
  const desktop = useMedia('(min-width: 768px)', true);
  // Made once per basemap file, not per refetch: a new one would make a new map.
  const basemapKey = JSON.stringify(index.data?.basemap ?? null);
  // biome-ignore lint/correctness/useExhaustiveDependencies: the key stands for the basemap's header
  const basemap = useMemo(() => basemapFor(index.data?.basemap ?? null), [basemapKey]);
  const [chosen, setChosen] = useState<string | null>(null);
  const choice = chooseLayers(index.data?.layers ?? [], chosen);
  const frames = choice.frames;
  const [picked, setTime] = useState<number | null>(null);
  const time = picked ?? startTime(frames, now);
  const [playing, setPlaying] = useState(false);
  usePlayback(playing, frames, time, setTime);
  const base = useWindPair(choice.base?.frames ?? [], time, playing);
  const detail = useWindPair(hoursAt(choice.detail?.frames ?? [], time), time, playing);
  const layers = useMemo(() => ({ base, detail }), [base, detail]);
  const [pick, setPick] = useState<Pick | null>(null);

  const modelOf = (id: string | undefined) => models.data?.find((x) => x.id === id);
  const nameOf = (id: string) => modelOf(id)?.name ?? id;
  const ready = index.data?.layers.filter((l) => l.frames.length > 0) ?? [];
  // The model that draws the hour shown where it is finest, for its run
  const drawing = detail ? choice.detail : choice.base;
  const drawingFrames = drawing?.frames ?? [];
  const frame = time !== null && drawingFrames.length ? drawingFrames[positionAt(drawingFrames, time).i] : null;
  const station = pick?.slug ? (pins.data?.find((p) => p.slug === pick.slug) ?? null) : null;
  const timeLabel = time !== null ? frameTime(time) : '';
  const empty = index.isSuccess && !choice.selected;
  const runLine = frame && drawing && (
    <>
      {nameOf(drawing.model)} ·{' '}
      {m.map_run({ run: runHour(frame.runAt), provider: modelOf(drawing.model)?.provider ?? '' })}
    </>
  );

  const chips = ready.length > 0 && (
    <fieldset aria-label={m.map_model()} className="m-0 flex min-w-0 flex-wrap gap-2 border-0 p-0">
      {ready.map((l) => {
        const md = modelOf(l.model);
        const on = l.model === choice.selected?.model;
        return (
          <button
            key={l.model}
            type="button"
            aria-pressed={on}
            onClick={() => setChosen(l.model)}
            className={cn(
              'inline-flex cursor-pointer items-baseline gap-1.5 rounded-md border px-2.5 py-1 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring/60',
              on ? 'border-border-strong bg-secondary text-foreground' : 'text-muted-foreground hover:text-foreground',
            )}
          >
            <span className="font-semibold">{nameOf(l.model)}</span>
            {md && (
              <span className="font-mono text-muted-foreground text-xs">
                {formatNumber(md.resolutionKm, md.resolutionKm < 10 ? 1 : 0)} km
              </span>
            )}
          </button>
        );
      })}
    </fieldset>
  );

  const modelInfo = ready.length > 0 && (
    <div className="flex flex-col gap-2">
      <div className="flex items-baseline justify-between gap-3">
        <h2 className="m-0 font-semibold text-sm">{m.map_model()}</h2>
        {detail && <span className="text-muted-foreground text-xs">{m.map_domainNote()}</span>}
      </div>
      {chips}
      {choice.detail && choice.base && (
        <p className="m-0 text-muted-foreground text-xs">
          {m.map_layered({ fine: nameOf(choice.detail.model), base: nameOf(choice.base.model) })}
        </p>
      )}
      {runLine && <p className="m-0 font-mono text-muted-foreground text-xs">{runLine}</p>}
    </div>
  );

  const spot = pick && (
    <SpotCard
      pick={pick}
      station={station}
      layers={layers}
      modelName={nameOf}
      unit={unit}
      onClose={() => setPick(null)}
    />
  );

  return (
    <main className="grid min-h-0 flex-1 md:grid-cols-[320px_minmax(0,1fr)]">
      {desktop && (
        <aside className="flex min-h-0 flex-col gap-6 overflow-y-auto border-line border-r p-5">
          <div className="flex flex-col gap-1">
            <p className="m-0 font-mono text-muted-foreground text-xs uppercase tracking-[0.08em]">{m.map_eyebrow()}</p>
            <h1 className="m-0 font-bold text-[32px] leading-tight tracking-[-0.04em]">{m.map_title()}</h1>
            {timeLabel && <p className="m-0 font-mono text-muted-foreground text-sm">{timeLabel}</p>}
          </div>
          {modelInfo}
          <div className="flex flex-col gap-2">
            <h2 className="m-0 font-semibold text-sm">{m.map_speed()}</h2>
            <Legend unit={unit} />
          </div>
          {spot ?? <p className="m-0 text-muted-foreground text-sm">{m.map_hint()}</p>}
          <Attribution osm={!!index.data?.basemap} className="mt-auto" />
        </aside>
      )}
      <div className="relative min-h-[420px]">
        {index.isPending ? (
          <div className="h-full w-full animate-pulse bg-muted" />
        ) : (
          <MapCanvas
            basemap={basemap}
            layers={layers}
            domain={detail ? (detail.a.meta.domain as [number, number][]) : null}
            pins={pins.data}
            unit={unit}
            picked={pick}
            onPick={setPick}
          />
        )}
        {!desktop && (
          <div className="absolute top-3 left-3 flex w-[min(260px,calc(100%-76px))] flex-col gap-2 rounded-xl border bg-card/95 p-3 backdrop-blur">
            <div className="flex flex-col">
              <h1 className="m-0 truncate font-semibold text-sm">{m.map_title()}</h1>
              {runLine && <p className="m-0 truncate font-mono text-muted-foreground text-xs">{runLine}</p>}
            </div>
            {ready.length > 1 && chips}
            <Legend unit={unit} />
          </div>
        )}
        {empty && (
          <p
            role="status"
            className="absolute top-1/2 left-1/2 m-0 w-[min(420px,calc(100%-32px))] -translate-x-1/2 -translate-y-1/2 rounded-xl border bg-card/95 p-5 text-center text-sm backdrop-blur"
          >
            {m.map_empty()}
          </p>
        )}
        <div className="absolute right-3 bottom-3 left-3 flex flex-col gap-2">
          {!desktop && spot}
          {frames.length > 0 && time !== null && (
            <Timeline frames={frames} time={time} now={now} playing={playing} onTime={setTime} onPlay={setPlaying} />
          )}
          {!desktop && <Attribution osm={!!index.data?.basemap} className="text-right" />}
        </div>
      </div>
    </main>
  );
}

function Attribution({ osm, className }: { osm: boolean; className?: string }) {
  const link = (href: string, label: string) => (
    <a href={href} className="text-subtle hover:text-foreground">
      {label}
    </a>
  );
  return (
    <p className={cn('m-0 text-[11px] text-muted-foreground leading-snug', className)}>
      {m.map_attribution()} {link('https://opendata.dwd.de/', 'DWD')} ·{' '}
      {link('https://www.ecmwf.int/en/forecasts/datasets/open-data', 'ECMWF')} (CC BY 4.0) ·{' '}
      {osm ? (
        <>
          {link('https://www.openstreetmap.org/copyright', '© OpenStreetMap')} ·{' '}
          {link('https://protomaps.com', 'Protomaps')}
        </>
      ) : (
        link('https://www.naturalearthdata.com/', 'Natural Earth')
      )}{' '}
      · {link('https://maplibre.org/', 'MapLibre')}
    </p>
  );
}
