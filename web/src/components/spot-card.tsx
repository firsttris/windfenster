/** A spot on the start page at a glance: the wind measured there now (stations), its coming days
 * as one band in the wind scale's colours (hours from directions that do not work at the spot
 * faded), and its strongest hour. From the wind map's data, nothing to set. */
import { useQuery } from '@tanstack/react-query';
import { Link } from '@tanstack/react-router';
import { Pencil } from 'lucide-react';
import type { Favorite } from '@/bindings/Favorite';
import { mapIndexQuery, mapSeriesQuery, mapStationsQuery } from '@/lib/api';
import { ago, compass, coordinates } from '@/lib/format';
import { gearFor, sizeText } from '@/lib/gear';
import { intlLocale } from '@/lib/i18n';
import { useUnit } from '@/lib/prefs';
import { glanceBand, peak } from '@/lib/spot-glance';
import { spotHours } from '@/lib/spot-series';
import { daySegments } from '@/lib/timeline';
import { formatWind, UNIT_LABEL } from '@/lib/units';
import { useGear } from '@/lib/use-gear';
import { useNow } from '@/lib/use-now';
import { BASE_MODELS } from '@/lib/wind-map';
import { m } from '@/paraglide/messages';
import { WindArrow } from '@/ui/wind-arrow';
import { windBand } from '@/ui/wind-scale';
import { KIND_NAME } from './gear-fit';

/** How far the band reaches */
const DAYS = 5;

const dayFmt = () => new Intl.DateTimeFormat(intlLocale(), { weekday: 'short' });
const whenFmt = () =>
  new Intl.DateTimeFormat(intlLocale(), { weekday: 'short', hour: '2-digit', minute: '2-digit', hourCycle: 'h23' });

export function SpotCard({ favorite: f, onEdit }: { favorite: Favorite; onEdit: () => void }) {
  const [unit] = useUnit();
  const now = useNow();
  const { gear } = useGear();
  const series = useQuery(mapSeriesQuery(f.lat, f.lon));
  const index = useQuery(mapIndexQuery);
  const pins = useQuery({ ...mapStationsQuery, enabled: f.slug !== null });
  const live = f.slug ? pins.data?.find((p) => p.slug === f.slug && p.wind !== null) : undefined;

  const detail = index.data?.layers.find((l) => !BASE_MODELS.includes(l.model))?.model ?? null;
  const hours = series.data ? spotHours(series.data, BASE_MODELS[0], detail) : [];
  const from = now - (now % 3600);
  const to = Math.min(from + DAYS * 86_400, hours.at(-1)?.validAt ?? from);
  const best = peak(hours, f.sector, now);
  const take = best ? gearFor(gear, best.wind, best.gust) : null;
  const pct = (t: number) => `${((t - from) / Math.max(1, to - from)) * 100}%`;
  const day = dayFmt();

  return (
    <article className="relative flex h-full flex-col gap-3 rounded-xl border bg-card p-4 transition-colors hover:border-border-strong">
      <Link
        {...(f.slug
          ? { to: '/station/$slug', params: { slug: f.slug } }
          : { to: '/point', search: { lat: f.lat, lon: f.lon, name: f.name } })}
        className="flex flex-col gap-0.5 pr-10 text-foreground no-underline after:absolute after:inset-0 after:content-['']"
      >
        <span className="truncate font-bold text-lg tracking-[-0.03em]">{f.name}</span>
        <span className="font-mono text-muted-foreground text-xs">
          {f.slug ? `${m.home_station()} ${f.slug.toUpperCase()}` : coordinates(f.lat, f.lon)}
          {f.sector && ` · ${m.favorite_sectorText({ from: compass(f.sector.from), to: compass(f.sector.to) })}`}
        </span>
      </Link>
      <button
        type="button"
        aria-label={`${m.home_favoritesEdit()}: ${f.name}`}
        onClick={onEdit}
        className="absolute top-3 right-3 z-10 inline-flex size-8 cursor-pointer items-center justify-center rounded-md text-muted-foreground outline-none hover:bg-secondary hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/60"
      >
        <Pencil aria-hidden="true" className="size-4" />
      </button>

      {live?.wind != null && (
        <p className="m-0 flex items-center gap-2 text-sm">
          {live.dir !== null && <WindArrow deg={live.dir} className="size-4" />}
          <span
            className="rounded px-1.5 font-mono font-semibold"
            style={{ background: windBand(live.wind).bg, color: windBand(live.wind).fg }}
          >
            {formatWind(live.wind, unit)}
          </span>
          <span className="text-muted-foreground">
            {UNIT_LABEL[unit]}
            {live.gust !== null && ` · ${m.spot_gusts({ gust: formatWind(live.gust, unit) })}`}
            {live.at !== null && ` · ${m.spot_measured({ ago: ago(live.at, now) })}`}
          </span>
        </p>
      )}

      {hours.length > 0 && to > from ? (
        <div className="flex flex-col gap-1" aria-hidden="true">
          <div
            className="relative h-3.5 overflow-hidden rounded-full"
            style={{ background: glanceBand(hours, from, to, f.sector) }}
          />
          <div className="relative h-4 font-mono text-[10px] text-muted-foreground">
            {daySegments(from, to).map((d) => (
              <span
                key={d.day}
                className="absolute top-0 truncate border-line border-l pl-1"
                style={{ left: pct(d.from), width: `calc(${pct(d.to)} - ${pct(d.from)})` }}
              >
                {day.format(new Date(d.day * 1000))}
              </span>
            ))}
          </div>
        </div>
      ) : (
        <div className="h-[34px] animate-pulse rounded-full bg-muted" />
      )}

      {best && (
        <p className="m-0 text-sm">
          <span className="text-muted-foreground">{m.spot_strongest()} </span>
          <span className="font-medium">{whenFmt().format(new Date(best.validAt * 1000))}</span>
          {' · '}
          <span className="font-mono font-semibold">
            {formatWind(best.wind, unit)} {UNIT_LABEL[unit]}
          </span>
          <span className="text-muted-foreground"> {m.spot_from({ dir: compass(best.dir) })}</span>
          {take !== null && typeof take === 'object' && (
            <span className="text-muted-foreground">
              {' · '}
              <span className="font-medium text-foreground">
                {KIND_NAME[take.fits.kind]()} {sizeText(take.fits.size)}
              </span>
              {take.gusty && ` (${m.gear_gusty()})`}
            </span>
          )}
        </p>
      )}
    </article>
  );
}
