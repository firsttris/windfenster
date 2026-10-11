import { cn } from '@/lib/utils';

interface SegmentedProps<T extends string> {
  label: string;
  value: T;
  options: readonly { value: T; label: string; disabled?: boolean }[];
  onChange: (value: T) => void;
  className?: string;
  /** Buttons share the full width (phones). */
  stretch?: boolean;
}

/** Toggle between a few options, like Superforecast / Forecast or the wind unit. */
export function Segmented<T extends string>({
  label,
  value,
  options,
  onChange,
  className,
  stretch = false,
}: SegmentedProps<T>) {
  return (
    <fieldset
      aria-label={label}
      className={cn('m-0 inline-flex min-w-0 gap-1 rounded-lg border bg-card p-1', className)}
    >
      {options.map((o) => {
        const on = o.value === value;
        return (
          <button
            key={o.value}
            type="button"
            aria-pressed={on}
            disabled={o.disabled}
            onClick={() => onChange(o.value)}
            className={cn(
              'h-9 min-w-11 cursor-pointer rounded-[9px] px-3 font-semibold text-sm outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring/60 disabled:cursor-not-allowed disabled:opacity-40',
              on ? 'bg-primary text-primary-foreground' : 'text-muted-foreground hover:text-foreground',
              stretch && 'flex-1',
            )}
          >
            {o.label}
          </button>
        );
      })}
    </fieldset>
  );
}
