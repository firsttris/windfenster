import { cn } from '@/lib/utils';
import { m } from '@/paraglide/messages';

/** What the colour field shows */
export type FieldLayer = 'wind' | 'gusts';

/** Wind or gusts, as two segments. */
export function LayerSwitch({
  value,
  onChange,
  compact = false,
  className,
}: {
  value: FieldLayer;
  onChange: (l: FieldLayer) => void;
  /** Smaller, for the card over the map on phones */
  compact?: boolean;
  className?: string;
}) {
  const options: [FieldLayer, string][] = [
    ['wind', m.map_layerWind()],
    ['gusts', m.map_layerGusts()],
  ];
  return (
    <fieldset
      aria-label={m.map_layer()}
      className={cn('m-0 inline-flex min-w-0 rounded-md border bg-background/60 p-0.5', className)}
    >
      {options.map(([id, label]) => (
        <button
          key={id}
          type="button"
          aria-pressed={value === id}
          onClick={() => onChange(id)}
          className={cn(
            'flex-1 cursor-pointer rounded-[5px] font-medium outline-none focus-visible:ring-2 focus-visible:ring-ring/60',
            compact ? 'px-2 py-0.5 text-xs' : 'px-3 py-1 text-sm',
            value === id ? 'bg-secondary text-foreground shadow-sm' : 'text-muted-foreground hover:text-foreground',
          )}
        >
          {label}
        </button>
      ))}
    </fieldset>
  );
}
