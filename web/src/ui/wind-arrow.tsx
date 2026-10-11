import { cn } from '@/lib/utils';

/** An arrow pointing where the wind blows to; `deg` is where it comes from (meteorological). */
export function WindArrow({ deg, className }: { deg: number; className?: string }) {
  return (
    <svg
      viewBox="0 0 24 24"
      aria-hidden="true"
      className={cn('size-[18px] shrink-0', className)}
      style={{ transform: `rotate(${Math.round(deg + 180) % 360}deg)` }}
    >
      <path d="M12 3 L18.5 20 L12 16 L5.5 20 Z" fill="currentColor" />
    </svg>
  );
}
