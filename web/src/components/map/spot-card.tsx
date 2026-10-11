import { Link } from '@tanstack/react-router';
import { ArrowRight, X } from 'lucide-react';
import { useState } from 'react';
import type { StationWind } from '@/bindings/StationWind';
import type { WindUnit } from '@/bindings/WindUnit';
import { ago, compass, coordinates } from '@/lib/format';
import type { SpotHour } from '@/lib/spot-series';
import { formatWind, UNIT_LABEL } from '@/lib/units';
import { cn } from '@/lib/utils';
import { gustOf, speedAndDirection } from '@/lib/wind-map';
import { m } from '@/paraglide/messages';
import { Button } from '@/ui/button';
import { WindArrow } from '@/ui/wind-arrow';
import { windBand } from '@/ui/wind-scale';
import type { Pick } from './map-canvas';
import { MeasuredSpark } from './measured-spark';
import { SpotStrip } from './spot-strip';
import { useSheetDrag } from './use-sheet-drag';
import { windAt } from './wind-at';
import type { WindLayers } from './wind-gl';

interface Props {
  pick: Pick;
  station: StationWind | null;
  layers: WindLayers;
  /** Model names by id */
  modelName: (id: string) => string;
  unit: WindUnit;
  /** The place's coming hours; undefined while they load */
  hours: readonly SpotHour[] | undefined;
  /** The hour the map shows */
  time: number | null;
  onTime: (t: number) => void;
  onClose: () => void;
  /** Tighter, for the phone's map */
  compact?: boolean;
  className?: string;
}

/** What the map knows about a tapped spot: the model's wind there at the hour shown, and for a
 * station its last measurement. */
export function SpotCard({
  pick,
  station,
  layers,
  modelName,
  unit,
  hours,
  time,
  onTime,
  onClose,
  compact = false,
  className,
}: Props) {
  // On phones a sheet: folded it shows the hours, pulled up everything (use-sheet-drag.ts)
  const [expanded, setExpanded] = useState(false);
  const sheet = useSheetDrag(expanded, setExpanded, onClose);
  const full = !compact || expanded;
  const at = windAt(layers, pick.lon, pick.lat);
  const wind = at && speedAndDirection(at.wind);
  // The model's gust, when its hour has one
  const gust = at && at.wind.gust !== null ? gustOf(at.wind) : null;
  return (
    <section
      aria-label={station?.name ?? m.map_spot()}
      className={cn(
        'flex flex-col rounded-xl border bg-card/95 backdrop-blur',
        compact ? 'gap-2 p-3 pt-1' : 'gap-3 p-4',
        sheet.dy === 0 && 'transition-transform duration-200',
        className,
      )}
      style={compact && sheet.dy !== 0 ? { transform: `translateY(${sheet.dy}px)` } : undefined}
    >
      {compact && (
        <button
          type="button"
          aria-label={expanded ? m.map_sheetLess() : m.map_sheetMore()}
          aria-expanded={expanded}
          onClick={() => !sheet.dragged() && setExpanded(!expanded)}
          data-grip="true"
          {...sheet.handlers}
          className="-mx-3 flex h-5 cursor-grab touch-none items-center justify-center outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
        >
          <span className="h-1 w-10 rounded-full bg-border-strong" />
        </button>
      )}
      <div
        className={cn('flex items-start justify-between gap-3', compact && 'touch-none')}
        {...(compact ? sheet.handlers : {})}
      >
        <div className="min-w-0">
          <h2 className="m-0 truncate font-semibold text-base tracking-[-0.01em]">
            {station ? station.name : m.map_spot()}{' '}
            {station && <span className="font-mono font-normal text-muted-foreground text-sm">{station.code}</span>}
          </h2>
          {full && <p className="m-0 font-mono text-muted-foreground text-xs">{coordinates(pick.lat, pick.lon)}</p>}
        </div>
        <Button variant="ghost" size="icon" className="-mt-2 -mr-2 size-8" aria-label={m.map_close()} onClick={onClose}>
          <X aria-hidden="true" />
        </Button>
      </div>

      <div className="flex flex-col gap-2 text-sm">
        <div className="flex items-center justify-between gap-3">
          <span className="truncate text-muted-foreground">{at ? modelName(at.model) : m.map_model()}</span>
          {wind ? (
            <span className="flex shrink-0 items-center gap-1.5 font-semibold">
              <WindArrow deg={wind.from} className="size-4" />
              <span
                className="rounded px-1.5 font-mono"
                style={{ background: windBand(wind.kn).bg, color: windBand(wind.kn).fg }}
              >
                {formatWind(wind.kn, unit)}
              </span>
              <span className="font-normal text-muted-foreground text-xs">
                {gust !== null
                  ? m.map_gust({ gust: formatWind(gust, unit), unit: UNIT_LABEL[unit] })
                  : UNIT_LABEL[unit]}{' '}
                · {compass(wind.from)}
              </span>
            </span>
          ) : (
            <span className="text-muted-foreground">{m.map_outside()}</span>
          )}
        </div>
        {hours === undefined ? (
          <div className="h-[86px] animate-pulse rounded-md bg-muted" />
        ) : (
          hours.length > 0 && <SpotStrip hours={hours} time={time} unit={unit} onTime={onTime} />
        )}
        {full && station && (
          <div className="flex items-center justify-between gap-3">
            <span className="text-muted-foreground">
              {station.at !== null ? m.map_measured({ ago: ago(station.at) }) : m.map_noMeasurement()}
            </span>
            {station.wind !== null && (
              <span className="flex shrink-0 items-center gap-1.5 font-semibold">
                {station.dir !== null && <WindArrow deg={station.dir} className="size-4" />}
                <span
                  className="rounded px-1.5 font-mono"
                  style={{ background: windBand(station.wind).bg, color: windBand(station.wind).fg }}
                >
                  {formatWind(station.wind, unit)}
                </span>
                <span className="font-normal text-muted-foreground text-xs">
                  {station.gust !== null
                    ? m.map_gust({ gust: formatWind(station.gust, unit), unit: UNIT_LABEL[unit] })
                    : UNIT_LABEL[unit]}
                </span>
              </span>
            )}
          </div>
        )}
        {full && station && station.at !== null && <MeasuredSpark slug={station.slug} unit={unit} />}
      </div>

      {full && (
        <Button asChild size={compact ? 'sm' : 'default'} className="w-full">
          {station ? (
            <Link to="/station/$slug" params={{ slug: station.slug }}>
              {m.map_openStation()} <ArrowRight aria-hidden="true" />
            </Link>
          ) : (
            <Link to="/point" search={{ lat: Number(pick.lat.toFixed(4)), lon: Number(pick.lon.toFixed(4)) }}>
              {m.map_forecastHere()} <ArrowRight aria-hidden="true" />
            </Link>
          )}
        </Button>
      )}
    </section>
  );
}
