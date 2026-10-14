/** Pieces every tab of the admin page uses: the head, state chips, a row of label and value. */
import type { ReactNode } from 'react';
import { cn } from '@/lib/utils';

export function TabHead({ title, lead, children }: { title: string; lead?: ReactNode; children?: ReactNode }) {
  return (
    <div className="flex flex-wrap items-end justify-between gap-4">
      <div className="flex flex-col gap-2">
        <h1 className="m-0 font-bold text-4xl leading-[1.05] tracking-[-0.05em]">{title}</h1>
        {lead && <p className="m-0 text-muted-foreground text-sm">{lead}</p>}
      </div>
      {children}
    </div>
  );
}

export type Tone = 'ok' | 'warn' | 'error' | 'running' | 'off';

const TONES: Record<Tone, string> = {
  ok: 'border-success-border bg-success-surface text-success-foreground',
  warn: 'border-warning-border bg-warning-surface text-warning-foreground',
  error: 'border-destructive/40 bg-destructive/10 text-destructive',
  running: 'border-chart-1/40 bg-chart-1/10 text-chart-1',
  off: 'border-border bg-secondary text-muted-foreground',
};

/** A state in a few words: "ok", "Fehler", "läuft" ... */
export function Chip({ tone, children, className }: { tone: Tone; children: ReactNode; className?: string }) {
  return (
    <span
      className={cn(
        'inline-flex items-center whitespace-nowrap rounded-full border px-2.5 py-0.5 font-medium text-xs',
        TONES[tone],
        className,
      )}
    >
      {children}
    </span>
  );
}

/** A label and its value on one line. */
export function Row({ label, children }: { label: ReactNode; children: ReactNode }) {
  return (
    <div className="flex items-baseline justify-between gap-3 text-sm">
      <span className="shrink-0 whitespace-nowrap text-muted-foreground">{label}</span>
      <span className="min-w-0 break-words text-right">{children}</span>
    </div>
  );
}

/** A table that scrolls sideways on phones; `cols` are the grid's column widths. */
export function Table({ cols, head, children }: { cols: string; head: ReactNode[]; children: ReactNode }) {
  return (
    <div className="overflow-x-auto">
      <div className="min-w-[600px]" style={{ ['--cols' as string]: cols }}>
        <div className="grid grid-cols-[var(--cols)] gap-3 border-line border-b px-2.5 pb-2 text-[11px] text-muted-foreground uppercase tracking-[0.08em]">
          {head.map((h, i) => (
            // biome-ignore lint/suspicious/noArrayIndexKey: fixed columns
            <span key={i}>{h}</span>
          ))}
        </div>
        {children}
      </div>
    </div>
  );
}

export const ROW = 'grid grid-cols-[var(--cols)] items-center gap-3 border-line border-b p-2.5 text-[13px]';
