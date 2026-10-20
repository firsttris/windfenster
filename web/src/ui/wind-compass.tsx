import type { Sector } from '@/bindings/Sector';
import { arc, at, xy } from '@/lib/compass-rose';
import { inSector } from '@/lib/favorites';
import { compass } from '@/lib/format';
import { cn } from '@/lib/utils';
import { m } from '@/paraglide/messages';

const C = 60;
/** Every 10°, longer every 90° */
const TICKS = Array.from({ length: 36 }, (_, i) => i * 10);

interface Props {
  /** Where the wind comes from, degrees; null when variable or calm */
  dir: number | null;
  /** No wind at all: nothing points */
  calm: boolean;
  /** The range the wind varied in (the METAR's `dddVddd`) */
  range?: { from: number; to: number } | null;
  /** The directions that work at the saved spot */
  sector?: Sector | null;
  className?: string;
}

/** A compass rose with the wind: an arrow from where it comes to where it blows (green when that
 * works at the spot), the range it varied in as a band inside, and the spot's good directions as a
 * green arc outside. VRB or calm stand in the middle instead. */
export function WindCompass({ dir, calm, range, sector, className }: Props) {
  const points = m.compass_points().split(',');
  const fits = sector && dir !== null ? inSector(sector, dir) : null;
  const label = [
    dir !== null ? m.live_compassFrom({ dir: compass(dir) }) : calm ? m.live_calm() : m.live_variable(),
    range && m.live_compassRange({ from: compass(range.from), to: compass(range.to) }),
    sector && m.live_compassSector({ from: compass(sector.from), to: compass(sector.to) }),
  ]
    .filter(Boolean)
    .join(' · ');
  return (
    <svg
      viewBox="0 0 120 120"
      role="img"
      aria-label={`${m.live_compass()}: ${label}`}
      className={cn('shrink-0', className)}
    >
      <circle cx={C} cy={C} r="58" className="fill-background stroke-border" strokeWidth="1" />
      {TICKS.map((d) => {
        const [x1, y1] = xy(d, d % 90 === 0 ? 47 : 51, C);
        const [x2, y2] = xy(d, 55, C);
        return (
          <line
            key={d}
            x1={x1}
            y1={y1}
            x2={x2}
            y2={y2}
            className={d % 90 === 0 ? 'stroke-subtle' : 'stroke-line'}
            strokeWidth={d % 90 === 0 ? 1.5 : 1}
          />
        );
      })}
      {sector && (
        <path
          d={arc(sector.from, sector.to, 53, C)}
          className="fill-none stroke-success"
          strokeWidth="4"
          strokeLinecap="round"
        />
      )}
      <circle cx={C} cy={C} r="41" className="fill-none stroke-line" strokeWidth="1" strokeDasharray="1.5 3" />
      {range && (
        <path
          d={arc(range.from, range.to, 41, C)}
          className="fill-none stroke-muted-foreground opacity-60"
          strokeWidth="3"
          strokeLinecap="round"
        />
      )}
      {[0, 90, 180, 270].map((d, i) => {
        const [x, y] = xy(d, 31, C);
        return (
          <text
            key={d}
            x={x}
            y={y}
            dy="0.35em"
            textAnchor="middle"
            className={cn('font-semibold text-[11px]', d === 0 ? 'fill-foreground' : 'fill-muted-foreground')}
          >
            {points[i * 4]}
          </text>
        );
      })}
      {dir !== null && (
        // Like the arrow beside the number: from where the wind comes (the round end on the rim)
        // across the middle to where it blows (the tip)
        <g className={fits ? 'text-success' : 'text-foreground'}>
          <line
            x1={xy(dir, 39, C)[0]}
            y1={xy(dir, 39, C)[1]}
            x2={xy(dir + 180, 28, C)[0]}
            y2={xy(dir + 180, 28, C)[1]}
            stroke="currentColor"
            strokeWidth="2.5"
            strokeLinecap="round"
          />
          <circle cx={xy(dir, 39, C)[0]} cy={xy(dir, 39, C)[1]} r="3" fill="currentColor" />
          <path
            d={`M${at(dir + 180, 41, C)} L${at(dir + 166, 26, C)} L${at(dir + 194, 26, C)} Z`}
            fill="currentColor"
            stroke="currentColor"
            strokeWidth="1"
            strokeLinejoin="round"
          />
        </g>
      )}
      <circle cx={C} cy={C} r="3" className="fill-foreground" />
      {dir === null && (
        <text x={C} y={C + 16} textAnchor="middle" className="fill-subtle font-mono font-semibold text-[10px]">
          {calm ? '0' : 'VRB'}
        </text>
      )}
    </svg>
  );
}
