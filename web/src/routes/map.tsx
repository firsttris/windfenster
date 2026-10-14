import { useQuery, useQueryClient } from '@tanstack/react-query';
import { createFileRoute, Link } from '@tanstack/react-router';
import { useEffect, useMemo, useState } from 'react';
import type { MapFrame } from '@/bindings/MapFrame';
import { InHeader } from '@/components/header-slot';
import { basemapFor } from '@/components/map/basemap';

import { Legend } from '@/components/map/legend';
import { MapCanvas, type Pick } from '@/components/map/map-canvas';
import { PillSelect } from '@/components/map/pill-select';
import { withRelief } from '@/components/map/relief';
import { SpotCard } from '@/components/map/spot-card';
import { Timeline } from '@/components/map/timeline';
import { TimelineSlim } from '@/components/map/timeline-slim';
import { useGlide } from '@/components/map/use-glide';
import { useMapKeys } from '@/components/map/use-map-keys';
import { usePlayback, useWindPair } from '@/components/map/use-wind-pair';
import type { WindPair } from '@/components/map/wind-gl';
import { useSearchTarget } from '@/components/search';
import { mapGaugesQuery, mapIndexQuery, mapSeriesQuery, mapStationsQuery, modelsQuery } from '@/lib/api';
import { formatNumber, runHour } from '@/lib/format';
import { mapQuery, parseMapState, parseSpot } from '@/lib/map-url';
import { usePref, useUnit } from '@/lib/prefs';
import { spotHours, windGradient } from '@/lib/spot-series';
import { useMedia } from '@/lib/use-media';
import { useNow } from '@/lib/use-now';
import { useTitle } from '@/lib/use-title';
import { cn } from '@/lib/utils';
import {
  chooseLayers,
  framesFor,
  hoursAt,
  MAP_ONLY_MODELS,
  type Place,
  pieceFor,
  positionAt,
  startTime,
} from '@/lib/wind-map';
import { m } from '@/paraglide/messages';

/** What the colour field shows */
type FieldLayer = 'wind' | 'gusts';

const NO_DETAIL: [number, number, number, number][] = [];

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
  const gauges = useQuery(mapGaugesQuery);
  const models = useQuery(modelsQuery);
  const [unit] = useUnit();
  const now = useNow();
  const desktop = useMedia('(min-width: 768px)', true);
  // Wide enough for model and layer in the header's row, beside the menu
  const wide = useMedia('(min-width: 1280px)', true);
  // Made once per basemap file, not per refetch: a new one would make a new map. New detail
  // boxes only load the tiles again (MapCanvas `detail`).
  const info = index.data?.basemap ?? null;
  const relief = index.data?.terrain != null;
  const basemapKey = JSON.stringify([info && { ...info, detail: [] }, relief]);
  // biome-ignore lint/correctness/useExhaustiveDependencies: the key stands for the basemap's header
  const basemap = useMemo(() => {
    const b = basemapFor(index.data?.basemap ?? null);
    return relief ? withRelief(b) : b;
  }, [basemapKey]);
  // The address wins over what this browser remembers (lib/map-url.ts).
  const [initial] = useState(() => parseMapState(window.location.search));
  const [layerPref, setLayerPref] = usePref('wf.mapLayer');
  const [modelPref, setModelPref] = usePref('wf.mapModel');
  const [chosen, setChosenState] = useState<string | null>(initial.model ?? modelPref);
  const setChosen = (model: string) => {
    setChosenState(model);
    setModelPref(model);
  };
  // The map's middle and view, after each move
  const [center, setCenter] = useState<(Place & { view: [number, number, number, number] }) | null>(null);
  const choice = chooseLayers(index.data?.layers ?? [], chosen);
  // A model sent in pieces (ICON global): the piece around the view
  const piece = center ? pieceFor(center.view) : null;
  const midFrames = framesFor(choice.mid, piece);
  const detailFrames = framesFor(choice.detail, piece);
  const frames = choice.frames;
  const [picked, setTime] = useState<number | null>(initial.time ?? null);
  const time = picked ?? startTime(frames, now);
  const [playing, setPlaying] = useState(false);
  usePlayback(playing, frames, time, setTime);
  // The wind slides to a nearby hour picked instead of jumping
  const windTime = useGlide(time, playing);
  const base = useWindPair(choice.base?.frames ?? [], windTime, playing);
  const mid = useWindPair(hoursAt(midFrames, windTime), windTime, playing);
  const detail = useWindPair(hoursAt(detailFrames, windTime), windTime, playing);
  const layers = useMemo(() => ({ base, mid, detail }), [base, mid, detail]);
  const [pick, setPick] = useState<Pick | null>(() => parseSpot(window.location.search));
  // A search result stays on the map: fly there and show the spot.
  const [focus, setFocus] = useState<{ lon: number; lat: number } | null>(null);
  // A place looked at gets detail on the server within a minute or two: ask again then.
  const queryClient = useQueryClient();
  useEffect(() => {
    if (!pick) return;
    const t = setTimeout(() => void queryClient.invalidateQueries({ queryKey: mapIndexQuery.queryKey }), 100_000);
    return () => clearTimeout(t);
  }, [pick, queryClient]);
  useSearchTarget((hit) => {
    setPick({ lon: hit.lon, lat: hit.lat, slug: hit.slug });
    setFocus({ lon: hit.lon, lat: hit.lat });
  });
  const [field, setFieldState] = useState<FieldLayer>((initial.gusts ?? layerPref === 'gusts') ? 'gusts' : 'wind');
  const setField = (f: FieldLayer) => {
    setFieldState(f);
    setLayerPref(f);
  };
  const gusts = field === 'gusts';
  // Hour, layer and model in the address, for sharing; the hour once per hour while playing
  const urlHour = picked === null ? null : Math.round(picked / 3600) * 3600;
  useEffect(() => {
    const { pathname, search, hash } = window.location;
    const query = mapQuery(search, { time: urlHour, gusts, model: chosen });
    if (query !== search) window.history.replaceState(window.history.state, '', pathname + query + hash);
  }, [urlHour, gusts, chosen]);
  useMapKeys({
    togglePlay: () => setPlaying((p) => !p),
    step: (h) => {
      setPlaying(false);
      if (time === null || frames.length === 0) return;
      setTime(Math.min(frames[frames.length - 1].validAt, Math.max(frames[0].validAt, time + h * 3600)));
    },
    toggleGusts: () => setField(gusts ? 'wind' : 'gusts'),
    toNow: () => {
      setPlaying(false);
      setTime(null);
    },
    close: () => setPick(null),
  });
  // The place picked over the coming hours, from the map's own fields
  const series = useQuery({ ...mapSeriesQuery(pick?.lat ?? 0, pick?.lon ?? 0), enabled: pick !== null });
  const hours = series.data
    ? spotHours(series.data, [choice.detail?.model, choice.mid?.model, choice.base?.model]).filter(
        (h) => h.validAt >= now - 3_600,
      )
    : series.isError
      ? []
      : undefined;
  // An hour asked for whose texture is still on its way: the map shows the last one meanwhile.
  const behind = (pair: WindPair | null, list: readonly MapFrame[]) => {
    if (time === null || list.length === 0) return false;
    return pair?.a.path !== list[positionAt(list, time).i]?.path;
  };
  const loading =
    behind(base, choice.base?.frames ?? []) ||
    behind(mid, hoursAt(midFrames, time)) ||
    behind(detail, hoursAt(detailFrames, time));
  // A model hour without gusts (ECMWF's first, some late ones) shows the wind instead.
  const gustless = gusts
    ? [detail, mid, base].find((p) => p && p.a.meta.gustMax === null && p.b.meta.gustMax === null)
    : null;

  const modelOf = (id: string | undefined) =>
    models.data?.find((x) => x.id === id) ?? (id ? MAP_ONLY_MODELS[id] : undefined);
  const nameOf = (id: string) => modelOf(id)?.name ?? id;
  const ready = index.data?.layers.filter((l) => l.frames.length > 0) ?? [];
  const station = pick?.slug ? (pins.data?.find((p) => p.slug === pick.slug) ?? null) : null;
  const empty = index.isSuccess && !choice.selected;
  // The run of the hour shown, beside the model in its pill on wide screens
  const runOf = (list: readonly MapFrame[]) =>
    time !== null && list.length > 0 ? list[positionAt(list, time).i].runAt : null;

  // On phones: in the spot sheet, or alone in a sheet at the foot
  const timeline = frames.length > 0 && time !== null && (
    <Timeline
      frames={frames}
      time={time}
      now={now}
      playing={playing}
      onTime={setTime}
      onPlay={setPlaying}
      band={hours && windGradient(hours, frames[0].validAt, frames[frames.length - 1].validAt, gusts)}
      loading={loading && !playing}
      compact
      bare
    />
  );

  const spot = pick && (
    <SpotCard
      pick={pick}
      station={station}
      layers={layers}
      modelName={nameOf}
      unit={unit}
      hours={hours}
      time={time}
      onTime={(t) => {
        setPlaying(false);
        setTime(t);
      }}
      onClose={() => setPick(null)}
      compact={!desktop}
      footer={desktop ? undefined : timeline || undefined}
    />
  );

  return (
    // The map fills the window; header, pills, spot and timeline float over it (styles.css, glass)
    <main className="relative min-h-0 flex-1">
      <div className="absolute inset-0">
        {index.isPending ? (
          <div className="h-full w-full animate-pulse bg-muted" />
        ) : (
          <MapCanvas
            basemap={basemap}
            layers={layers}
            gusts={gusts}
            pins={pins.data}
            gauges={gauges.data}
            unit={unit}
            picked={pick}
            onPick={setPick}
            onCenter={(lon, lat, view) => setCenter({ lon, lat, view })}
            focus={focus}
            detail={info?.detail ?? NO_DETAIL}
          />
        )}
        {/* As in the mockups: model and layer as two pills, the scale slim beneath; on wide
            screens the pills move up into the header's row, and the spot picked lies under the
            scale (on phones it is the sheet at the foot) */}
        <div
          className={cn(
            'absolute flex flex-col items-start',
            desktop
              ? 'top-20 bottom-[120px] left-4 w-[320px] gap-2'
              : 'top-[76px] left-3 w-[min(300px,calc(100%-76px))] gap-1.5',
          )}
        >
          <h1 className="sr-only">{m.map_title()}</h1>
          <InHeader when={wide}>
            <div className={cn('flex items-center', desktop ? 'w-max gap-2' : 'max-w-full gap-1.5')}>
              {ready.length > 0 && (
                <PillSelect
                  label={m.map_model()}
                  value={choice.selected?.model ?? ready[0].model}
                  onChange={setChosen}
                  options={ready.map((l) => {
                    const md = modelOf(l.model);
                    const run = desktop ? runOf(l.frames) : null;
                    const parts = [
                      md && `${formatNumber(md.resolutionKm, md.resolutionKm < 10 ? 1 : 0)} km`,
                      run !== null && m.map_run({ run: runHour(run) }),
                    ].filter(Boolean);
                    return {
                      value: l.model,
                      label: nameOf(l.model),
                      detail: parts.length > 0 ? parts.join(' · ') : undefined,
                    };
                  })}
                  className={cn(desktop && 'h-10 px-3.5 text-sm')}
                />
              )}
              <PillSelect
                label={m.map_layer()}
                value={field}
                onChange={(f) => setField(f as FieldLayer)}
                options={[
                  { value: 'wind', label: m.map_layerWind() },
                  { value: 'gusts', label: m.map_layerGusts() },
                ]}
                className={cn('shrink-0', desktop && 'h-10 px-3.5 text-sm')}
              />
            </div>
          </InHeader>
          <div
            className={cn(
              'glass flex flex-col gap-1 rounded-[10px] border px-2.5 pt-1.5 pb-1 shadow-none',
              desktop ? 'w-[268px]' : 'w-[210px]',
            )}
          >
            <Legend unit={unit} gusts={gusts} compact />
            {gustless && (
              <p className="m-0 text-[11px] text-muted-foreground">
                {m.map_noGusts({ model: nameOf(gustless.a.meta.model) })}
              </p>
            )}
          </div>
          {desktop && spot && <div className="mt-1 min-h-0 w-full overflow-y-auto rounded-xl">{spot}</div>}
        </div>
        {empty && (
          <p
            role="status"
            className="absolute top-1/2 left-1/2 m-0 w-[min(420px,calc(100%-32px))] -translate-x-1/2 -translate-y-1/2 glass rounded-xl border p-5 text-center text-sm"
          >
            {m.map_empty()}
          </p>
        )}
        {/* On phones a sheet at the foot, edge to edge (the spot and the timeline in it), the
            sources above it. On wider screens the timeline lies on the map without a card, over a
            soft shade, and the sources sit in the corner. */}
        {desktop ? (
          <>
            <div
              aria-hidden="true"
              className="pointer-events-none absolute inset-x-0 bottom-0 h-[130px] bg-gradient-to-b from-black/0 to-black/45"
            />
            {frames.length > 0 && time !== null && (
              <TimelineSlim
                frames={frames}
                time={time}
                now={now}
                onTime={setTime}
                onPlay={setPlaying}
                band={hours && windGradient(hours, frames[0].validAt, frames[frames.length - 1].validAt, gusts)}
                className="absolute bottom-10 left-1/2 w-[min(860px,calc(100%-32px))] -translate-x-1/2"
              />
            )}
            <Attribution
              osm={!!index.data?.basemap}
              relief={relief}
              className="glass absolute right-3 bottom-2.5 w-fit max-w-[calc(100%-24px)] rounded-md border px-2 py-1 text-right shadow-none"
            />
          </>
        ) : (
          <div className="absolute inset-x-0 bottom-0 flex flex-col gap-2">
            <Attribution
              osm={!!index.data?.basemap}
              relief={relief}
              short
              className="glass mr-2 ml-auto w-fit max-w-full rounded-md border px-1.5 py-0.5 text-right text-[10px] shadow-none"
            />
            {spot || (
              <div className="glass rounded-t-[20px] rounded-b-none border border-x-0 border-b-0 px-4 pt-4 pb-[max(20px,env(safe-area-inset-bottom))] shadow-[0_-12px_32px_rgb(0_0_0/0.35)]">
                {timeline}
              </div>
            )}
          </div>
        )}
      </div>
    </main>
  );
}

/** The sources the map shows, with their licences; `short` on phones keeps the ones the licences
 * ask to name on the map itself (CC BY, OpenStreetMap) and leaves the rest to the sources page. */
function Attribution({
  osm,
  relief,
  short = false,
  className,
}: {
  osm: boolean;
  relief: boolean;
  short?: boolean;
  className?: string;
}) {
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
      {relief && <>· {link('https://mapterhorn.com/attribution', '© Mapterhorn')} </>}·{' '}
      {!short && <>{link('https://maplibre.org/', 'MapLibre')} · </>}
      <Link to="/sources" className="text-subtle hover:text-foreground">
        {m.sources_all()}
      </Link>
    </p>
  );
}
