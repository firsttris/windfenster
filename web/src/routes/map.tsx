import { useQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { useState } from 'react';
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
import { positionAt, startTime } from '@/lib/wind-map';
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
  const layer = index.data?.layers.find((l) => l.frames.length > 0) ?? null;
  const frames = layer?.frames ?? [];
  const [picked, setTime] = useState<number | null>(null);
  const time = picked ?? startTime(frames, now);
  const [playing, setPlaying] = useState(false);
  usePlayback(playing, frames, time, setTime);
  const pair = useWindPair(frames, time, playing);
  const [pick, setPick] = useState<Pick | null>(null);

  const model = models.data?.find((x) => x.id === layer?.model);
  const modelName = model?.name ?? layer?.model ?? '';
  const frame = time !== null && frames.length ? frames[positionAt(frames, time).i] : null;
  const station = pick?.slug ? (pins.data?.find((p) => p.slug === pick.slug) ?? null) : null;
  const timeLabel = time !== null ? frameTime(time) : '';
  const empty = index.isSuccess && !layer;

  const modelInfo = layer && (
    <div className="flex flex-col gap-2">
      <div className="flex items-baseline justify-between gap-3">
        <h2 className="m-0 font-semibold text-sm">{m.map_model()}</h2>
        <span className="text-muted-foreground text-xs">{m.map_domainNote()}</span>
      </div>
      <div className="flex flex-wrap gap-2">
        <span className="inline-flex items-baseline gap-1.5 rounded-md border border-border-strong bg-secondary px-2.5 py-1 text-sm">
          <span className="font-semibold">{modelName}</span>
          {model && (
            <span className="font-mono text-muted-foreground text-xs">
              {formatNumber(model.resolutionKm, model.resolutionKm < 10 ? 1 : 0)} km
            </span>
          )}
        </span>
      </div>
      {frame && (
        <p className="m-0 font-mono text-muted-foreground text-xs">
          {m.map_run({ run: runHour(frame.runAt), provider: model?.provider ?? '' })}
        </p>
      )}
    </div>
  );

  const spot = pick && (
    <SpotCard pick={pick} station={station} pair={pair} model={modelName} unit={unit} onClose={() => setPick(null)} />
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
          <Attribution className="mt-auto" />
        </aside>
      )}
      <div className="relative min-h-[420px]">
        <MapCanvas
          pair={pair}
          domain={pair ? (pair.a.meta.domain as [number, number][]) : null}
          pins={pins.data}
          unit={unit}
          picked={pick}
          onPick={setPick}
        />
        {!desktop && (
          <div className="absolute top-3 left-3 flex w-[min(260px,calc(100%-76px))] flex-col gap-2 rounded-xl border bg-card/95 p-3 backdrop-blur">
            <div className="flex flex-col">
              <h1 className="m-0 truncate font-semibold text-sm">{m.map_title()}</h1>
              {layer && frame && (
                <p className="m-0 truncate font-mono text-muted-foreground text-xs">
                  {modelName} · {m.map_run({ run: runHour(frame.runAt), provider: model?.provider ?? '' })}
                </p>
              )}
            </div>
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
          {!desktop && <Attribution className="text-right" />}
        </div>
      </div>
    </main>
  );
}

function Attribution({ className }: { className?: string }) {
  return (
    <p className={cn('m-0 text-[11px] text-muted-foreground leading-snug', className)}>
      {m.map_attribution()}{' '}
      <a href="https://opendata.dwd.de/" className="text-subtle hover:text-foreground">
        DWD
      </a>{' '}
      ·{' '}
      <a href="https://www.naturalearthdata.com/" className="text-subtle hover:text-foreground">
        Natural Earth
      </a>{' '}
      ·{' '}
      <a href="https://maplibre.org/" className="text-subtle hover:text-foreground">
        MapLibre
      </a>
    </p>
  );
}
