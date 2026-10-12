import { useQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { FavoriteButton } from '@/components/favorite';
import { ForecastView } from '@/components/forecast/forecast-view';
import { HistoryCard } from '@/components/live/history-card';
import { LiveTile } from '@/components/live/live-tile';
import { StatsCard } from '@/components/live/stats-card';
import { MapLink } from '@/components/map-link';
import { Failed, Loading, Page } from '@/components/page';
import { WaterTile } from '@/components/water-tile';
import { liveQuery, stationQuery } from '@/lib/api';
import { coordinates, countryName, formatNumber } from '@/lib/format';
import { useTitle } from '@/lib/use-title';
import { m } from '@/paraglide/messages';
import { Badge } from '@/ui/badge';

export const Route = createFileRoute('/station/$slug')({
  loader: ({ context, params }) => {
    void context.queryClient.prefetchQuery(stationQuery(params.slug));
    void context.queryClient.prefetchQuery(liveQuery(params.slug));
  },
  component: StationPage,
});

const SOURCE = { metar: m.source_metar, dwd: m.source_dwd, owm: m.source_owm } as const;

function StationPage() {
  const { slug } = Route.useParams();
  const q = useQuery(stationQuery(slug));
  const live = useQuery(liveQuery(slug));
  useTitle(q.data ? m.title_station({ name: q.data.station.name, code: q.data.station.code }) : null);
  if (q.isPending) return <Loading />;
  if (q.isError) return <Failed error={q.error} />;
  const { station: st, forecast } = q.data;
  const place = [countryName(st.country), st.region].filter(Boolean);
  return (
    <Page>
      <section className="flex flex-col gap-2.5">
        <div className="flex flex-wrap items-center gap-2 text-muted-foreground text-[13px]">
          {place.map((p, i) => (
            <span key={p} className="flex items-center gap-2">
              {i > 0 && <span aria-hidden="true">/</span>}
              {p}
            </span>
          ))}
          <Badge className={place.length ? 'ml-1.5' : ''}>{SOURCE[st.source]()}</Badge>
        </div>
        <div className="flex flex-wrap items-end justify-between gap-3">
          <h1 className="m-0 font-bold text-[40px] leading-none tracking-[-0.05em] sm:text-6xl">
            {st.name}{' '}
            <span className="whitespace-nowrap font-mono font-semibold text-muted-foreground text-xl tracking-normal sm:text-[28px]">
              {st.code}
            </span>
          </h1>
          <div className="flex flex-wrap gap-2">
            <MapLink lat={st.lat} lon={st.lon} slug={st.slug} />
            <FavoriteButton target={{ slug: st.slug }} lat={st.lat} lon={st.lon} name={st.municipality ?? st.name} />
          </div>
        </div>
        <p className="m-0 font-mono text-muted-foreground text-[13px]">
          {coordinates(st.lat, st.lon)}
          {st.elevationM !== null && ` · ${formatNumber(st.elevationM)} m`}
          {st.municipality && ` · ${st.municipality}`}
        </p>
      </section>
      <ForecastView
        fc={forecast}
        tiles={
          <>
            <LiveTile station={st} timeZone={forecast.timezone ?? 'UTC'} />
            <WaterTile lat={st.lat} lon={st.lon} timeZone={forecast.timezone ?? 'UTC'} />
          </>
        }
        measured={
          live.data && {
            source: `${st.source === 'metar' ? 'METAR' : 'DWD'} ${st.code}`,
            observations: live.data.observations,
          }
        }
        taf={live.data?.taf}
        history={
          <>
            <HistoryCard station={st} timeZone={forecast.timezone ?? 'UTC'} />
            <StatsCard station={st} />
          </>
        }
        ranking={q.data.ranking}
        spot={{ slug: st.slug }}
        rankingStation={{ code: st.code, source: `${st.source === 'metar' ? 'METAR' : 'DWD'} ${st.code}` }}
      />
    </Page>
  );
}
