import { useQuery } from '@tanstack/react-query';
import { createFileRoute, Link } from '@tanstack/react-router';
import { ArrowRight } from 'lucide-react';
import { FavoriteButton } from '@/components/favorite';
import { ForecastView } from '@/components/forecast/forecast-view';
import { MapLink } from '@/components/map-link';
import { Failed, Loading, Page } from '@/components/page';
import { WaterTile } from '@/components/water-tile';
import { pointQuery } from '@/lib/api';
import { coordinates, km } from '@/lib/format';
import { useTitle } from '@/lib/use-title';
import { m } from '@/paraglide/messages';

interface PointSearch {
  lat: number;
  lon: number;
  /** Name of the place the search found, for the title. */
  name?: string;
}

export const Route = createFileRoute('/point')({
  validateSearch: (s: Record<string, unknown>): PointSearch => ({
    lat: Number(s.lat),
    lon: Number(s.lon),
    name: typeof s.name === 'string' && s.name ? s.name : undefined,
  }),
  loaderDeps: ({ search }) => ({ lat: search.lat, lon: search.lon }),
  loader: ({ context, deps }) => {
    if (Number.isFinite(deps.lat) && Number.isFinite(deps.lon)) {
      void context.queryClient.prefetchQuery(pointQuery(deps.lat, deps.lon));
    }
  },
  component: PointPage,
});

function PointPage() {
  const { lat, lon, name } = Route.useSearch();
  const q = useQuery({ ...pointQuery(lat, lon), enabled: Number.isFinite(lat) && Number.isFinite(lon) });
  const title = name ?? m.point_title();
  useTitle(m.title_point({ name: title }));
  if (!Number.isFinite(lat) || !Number.isFinite(lon)) return <Failed error={null} />;
  if (q.isPending) return <Loading />;
  if (q.isError) return <Failed error={q.error} />;
  const { forecast, nearest, ranking } = q.data;
  return (
    <Page>
      <section className="flex flex-col gap-2.5">
        <div className="flex flex-wrap items-end justify-between gap-3">
          <h1 className="m-0 font-bold text-[40px] leading-none tracking-[-0.05em] sm:text-6xl">{title}</h1>
          <div className="flex flex-wrap gap-2">
            <MapLink lat={lat} lon={lon} slug={null} />
            <FavoriteButton target={{ lat, lon }} lat={lat} lon={lon} name={title} />
          </div>
        </div>
        <p className="m-0 font-mono text-muted-foreground text-[13px]">{coordinates(lat, lon)}</p>
        {nearest && (
          <Link
            to="/station/$slug"
            params={{ slug: nearest.station.slug }}
            className="flex w-fit items-center gap-2 rounded-full border px-3 py-1.5 text-sm text-subtle hover:border-border-strong hover:text-foreground"
          >
            {m.point_nearest({
              name: nearest.station.name,
              code: nearest.station.code,
              distance: km(nearest.distanceKm),
            })}
            <ArrowRight className="size-4" aria-hidden="true" />
          </Link>
        )}
      </section>
      <ForecastView
        fc={forecast}
        tiles={<WaterTile lat={lat} lon={lon} timeZone={forecast.timezone ?? 'UTC'} />}
        ranking={ranking}
        spot={{ lat, lon }}
        rankingStation={
          nearest && ranking
            ? {
                code: nearest.station.code,
                source: `${nearest.station.source === 'metar' ? 'METAR' : 'DWD'} ${nearest.station.code}`,
                from: m.ranking_from({ name: nearest.station.name, distance: km(nearest.distanceKm) }),
              }
            : undefined
        }
      />
    </Page>
  );
}
