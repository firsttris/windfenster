import { cn } from '@/lib/utils';

/** The wind mark from the mockup: three gusts, the top two curling. */
export function WindMark({ className }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2.2"
      strokeLinecap="round"
      aria-hidden="true"
      className={className}
    >
      <path d="M3 8h11a3 3 0 1 0-3-3" />
      <path d="M3 12h16a3 3 0 1 1-3 3" />
      <path d="M3 16h7" />
    </svg>
  );
}

export function Logo({ className }: { className?: string }) {
  return (
    <span className={cn('flex items-center gap-2.5', className)}>
      <span className="flex size-8 items-center justify-center rounded-[9px] bg-primary text-primary-foreground">
        <WindMark className="size-5" />
      </span>
      {/* A notch smaller on the narrowest phones, so the header keeps its buttons in view */}
      <span className="font-bold text-[19px] tracking-[-0.05em] min-[380px]:text-[22px]">Windfenster</span>
    </span>
  );
}
