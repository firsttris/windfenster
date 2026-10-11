import type { WindUnit } from '@/bindings/WindUnit';
import { formatWind } from '@/lib/units';
import { cn } from '@/lib/utils';
import { windBand } from './wind-scale';

/** A forecast-table cell: the wind in the chosen unit on its scale colour. */
export function WindCell({ kn, unit, className }: { kn: number; unit: WindUnit; className?: string }) {
  const band = windBand(kn);
  return (
    <span
      className={cn(
        'flex h-[34px] w-[52px] items-center justify-center rounded-sm font-mono text-base font-bold',
        className,
      )}
      style={{ background: band.bg, color: band.fg }}
    >
      {formatWind(kn, unit)}
    </span>
  );
}
