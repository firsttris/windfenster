import { ChevronDown } from 'lucide-react';
import { cn } from '@/lib/utils';

interface Option {
  value: string;
  label: string;
  /** Beside the label in the pill, quieter (the model's resolution) */
  detail?: string;
}

/** A choice as a round button over the map on phones: the phone's own picker opens on a tap
 * (a native select laid over the pill, invisible). */
export function PillSelect({
  label,
  value,
  options,
  onChange,
  className,
}: {
  /** What is chosen, for screen readers */
  label: string;
  value: string;
  options: readonly Option[];
  onChange: (value: string) => void;
  className?: string;
}) {
  const shown = options.find((o) => o.value === value) ?? options[0];
  return (
    <span
      className={cn(
        'glass relative inline-flex h-9 min-w-0 items-center gap-1.5 rounded-full border px-3 font-semibold text-[13px] shadow-none has-[select:focus-visible]:ring-2 has-[select:focus-visible]:ring-ring/60',
        className,
      )}
    >
      <span className="truncate">{shown?.label}</span>
      {shown?.detail && <span className="shrink-0 font-normal text-muted-foreground">{shown.detail}</span>}
      <ChevronDown aria-hidden="true" className="size-3.5 shrink-0" />
      <select
        aria-label={label}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        className="absolute inset-0 cursor-pointer opacity-0"
      >
        {options.map((o) => (
          <option key={o.value} value={o.value}>
            {o.detail ? `${o.label} · ${o.detail}` : o.label}
          </option>
        ))}
      </select>
    </span>
  );
}
