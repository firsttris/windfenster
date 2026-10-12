/** Which of the user's gear fits an hour, as a table cell: the kind over the size, or that it is
 * too little or too much wind; a dot when the gusts go past the piece's top. */
import type { GearKind } from '@/bindings/GearKind';
import type { GearFit } from '@/lib/gear';
import { sizeText } from '@/lib/gear';
import { cn } from '@/lib/utils';
import { m } from '@/paraglide/messages';

export const KIND_NAME: Record<GearKind, () => string> = {
  kite: m.gear_kite,
  wing: m.gear_wing,
  windsurf: m.gear_windsurf,
  parawing: m.gear_parawing,
};

export function GearFitCell({ fit, className }: { fit: GearFit | null; className?: string }) {
  if (fit === null || fit === 'none' || fit === 'low') {
    return (
      <span className={cn('text-dim text-xs', className)} title={fit === 'low' ? m.gear_low() : undefined}>
        –
      </span>
    );
  }
  if (fit === 'high') {
    return <span className={cn('font-medium text-[10px] text-destructive', className)}>{m.gear_high()}</span>;
  }
  const { fits, gusty } = fit;
  return (
    <span
      className={cn('inline-flex flex-col items-center leading-tight', className)}
      title={gusty ? m.gear_gusty() : undefined}
    >
      <span className="text-[10px] text-muted-foreground">{KIND_NAME[fits.kind]()}</span>
      <span className="flex items-center gap-0.5 font-mono font-semibold text-[13px]">
        {sizeText(fits.size)}
        {gusty && (
          <span className="size-1.5 rounded-full bg-amber-500">
            <span className="sr-only">{m.gear_gusty()}</span>
          </span>
        )}
      </span>
    </span>
  );
}
