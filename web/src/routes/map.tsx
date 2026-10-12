import { useQuery, useQueryClient } from '@tanstack/react-query';
import { createFileRoute, Link } from '@tanstack/react-router';
import { PanelLeftClose, PanelLeftOpen } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import type { MapFrame } from '@/bindings/MapFrame';
import { basemapFor } from '@/components/map/basemap';
import { type FieldLayer, LayerSwitch } from '@/components/map/layer-switch';
import { Legend } from '@/components/map/legend';
import { MapCanvas, type Pick } from '@/components/map/map-canvas';
import { withRelief } from '@/components/map/relief';
import { ShareButton } from '@/components/map/share-button';
import { SpotCard } from '@/components/map/spot-card';
import { frameTime, Timeline } from '@/components/map/timeline';
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
  covers,
  framesFor,
  hoursAt,
  MAP_ONLY_MODELS,
  type Place,
  pieceFor,
  positionAt,
  startTime,
} from '@/lib/wind-map';
import { m } from '@/paraglide/messages';
import { Button } from '@/ui/button';

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
  // The desktop panel, folded away to see the whole map; a spot picked opens it again
  const [panelPref, setPanel] = usePref('wf.mapPanel');
  const panelOpen = panelPref !== 'closed';
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
  const [pick, setPickState] = useState<Pick | null>(() => parseSpot(window.location.search));
  const setPick = (p: Pick | null) => {
    setPickState(p);
    if (p && panelPref === 'closed') setPanel(null);
  };
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
  // The model that draws the hour shown at the spot picked or else in the middle of the view,
  // for its run: the finest whose texture holds the place.
  const where = pick ?? center;
  const drawing =
    (
      [
        [detail, choice.detail],
        [mid, choice.mid],
      ] as const
    ).find(([pair]) => pair !== null && (where === null || covers(pair.a.meta, where)))?.[1] ?? choice.base;
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

  // The models drawn, finest first
  const stack = [choice.detail, choice.mid, choice.base].flatMap((l) => (l ? [nameOf(l.model)] : []));
  const modelInfo = ready.length > 0 && (
    <div className="flex flex-col gap-2">
      <h2 className="m-0 font-semibold text-sm">{m.map_model()}</h2>
      {chips}
      {stack.length > 1 && (
        <p className="m-0 text-muted-foreground text-xs">{m.map_layered({ models: stack.join(' › ') })}</p>
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
      hours={hours}
      time={time}
      onTime={(t) => {
        setPlaying(false);
        setTime(t);
      }}
      onClose={() => setPick(null)}
      compact={!desktop}
    />
  );

  return (
    // The map fills the window; header, panel and timeline float over it (styles.css, glass)
    <main className="relative min-h-0 flex-1">
      {desktop && !panelOpen && (
        <Button
          variant="outline"
          size="icon"
          className="glass absolute top-[78px] left-3.5 z-20"
          aria-label={m.map_panelOpen()}
          aria-expanded={false}
          onClick={() => setPanel(null)}
        >
          <PanelLeftOpen aria-hidden="true" />
        </Button>
      )}
      {desktop && panelOpen && (
        // Below the 64 px header (routes/__root.tsx), above the timeline
        <aside className="glass absolute top-[78px] left-3.5 z-20 flex max-h-[calc(100%-240px)] w-[300px] flex-col gap-5 overflow-y-auto rounded-2xl border p-[18px]">
          <div className="flex flex-col gap-1">
            <div className="-my-2 flex items-center justify-between gap-2">
              <p className="m-0 font-mono text-muted-foreground text-xs uppercase tracking-[0.08em]">
                {m.map_eyebrow()}
              </p>
              <div className="-mr-2 flex shrink-0">
                <ShareButton className="size-8" />
                <Button
                  variant="ghost"
                  size="icon"
                  className="size-8"
                  aria-label={m.map_panelClose()}
                  aria-expanded={true}
                  onClick={() => setPanel('closed')}
                >
                  <PanelLeftClose aria-hidden="true" />
                </Button>
              </div>
            </div>
            <h1 className="m-0 font-bold text-[32px] leading-tight tracking-[-0.04em]">{m.map_title()}</h1>
            {timeLabel && <p className="m-0 font-mono text-muted-foreground text-sm">{timeLabel}</p>}
          </div>
          {modelInfo}
          <div className="flex flex-col gap-2">
            <h2 className="m-0 font-semibold text-sm">{gusts ? m.map_gustSpeed() : m.map_speed()}</h2>
            <LayerSwitch value={field} onChange={setField} />
            <Legend unit={unit} gusts={gusts} />
            {gustless && (
              <p className="m-0 text-muted-foreground text-xs">
                {m.map_noGusts({ model: nameOf(gustless.a.meta.model) })}
              </p>
            )}
          </div>
          {spot ?? <p className="m-0 text-muted-foreground text-sm">{m.map_hint()}</p>}
          <p className="m-0 text-[11px] text-muted-foreground leading-snug">{m.map_keys()}</p>
        </aside>
      )}
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
        {!desktop && (
          <div className="glass absolute top-[76px] left-3 flex w-[min(300px,calc(100%-76px))] flex-col gap-2 rounded-xl border p-2.5">
            <h1 className="sr-only">{m.map_title()}</h1>
            <div className="flex items-center gap-2">
              <p className="m-0 min-w-0 flex-1 truncate font-mono text-[11px] text-muted-foreground">{runLine}</p>
              <ShareButton className="-my-1.5 -mr-1.5 size-8 shrink-0" />
            </div>
            <div className="flex flex-wrap items-center gap-1.5">
              {ready.length > 1 && (
                <fieldset
                  aria-label={m.map_model()}
                  className="m-0 inline-flex min-w-0 rounded-md border bg-background/60 p-0.5"
                >
                  {ready.map((l) => (
                    <button
                      key={l.model}
                      type="button"
                      aria-pressed={l.model === choice.selected?.model}
                      onClick={() => setChosen(l.model)}
                      className={cn(
                        'cursor-pointer whitespace-nowrap rounded-[5px] px-2 py-0.5 font-medium text-xs outline-none focus-visible:ring-2 focus-visible:ring-ring/60',
                        l.model === choice.selected?.model
                          ? 'bg-secondary text-foreground shadow-sm'
                          : 'text-muted-foreground hover:text-foreground',
                      )}
                    >
                      {nameOf(l.model)}
                    </button>
                  ))}
                </fieldset>
              )}
              <LayerSwitch value={field} onChange={setField} compact />
            </div>
            <Legend unit={unit} gusts={gusts} />
            {gustless && (
              <p className="m-0 text-muted-foreground text-xs">
                {m.map_noGusts({ model: nameOf(gustless.a.meta.model) })}
              </p>
            )}
          </div>
        )}
        {empty && (
          <p
            role="status"
            className="absolute top-1/2 left-1/2 m-0 w-[min(420px,calc(100%-32px))] -translate-x-1/2 -translate-y-1/2 glass rounded-xl border p-5 text-center text-sm"
          >
            {m.map_empty()}
          </p>
        )}
        <div className="absolute right-3 bottom-3 left-3 flex flex-col gap-2">
          {!desktop && spot}
          {frames.length > 0 && time !== null && (
            <Timeline
              frames={frames}
              time={time}
              now={now}
              playing={playing}
              onTime={setTime}
              onPlay={setPlaying}
              band={hours && windGradient(hours, frames[0].validAt, frames[frames.length - 1].validAt, gusts)}
              loading={loading && !playing}
            />
          )}
          <Attribution
            osm={!!index.data?.basemap}
            relief={relief}
            className="glass ml-auto w-fit max-w-full rounded-md border px-2 py-1 text-right shadow-none"
          />
        </div>
      </div>
    </main>
  );
}

function Attribution({ osm, relief, className }: { osm: boolean; relief: boolean; className?: string }) {
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
      {link('https://www.pegelonline.wsv.de/', 'PEGELONLINE (WSV)')} · {link('https://maplibre.org/', 'MapLibre')} ·{' '}
      <Link to="/sources" className="text-subtle hover:text-foreground">
        {m.sources_all()}
      </Link>
    </p>
  );
}
