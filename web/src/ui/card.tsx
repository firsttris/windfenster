import type { ComponentProps } from 'react';
import { cn } from '@/lib/utils';

function Card({ className, ...props }: ComponentProps<'article'>) {
  return (
    <article
      data-slot="card"
      className={cn('flex flex-col gap-3.5 rounded-xl border bg-card p-5 text-card-foreground', className)}
      {...props}
    />
  );
}

/** Label row on top of a card: title left, a quiet detail (age, source) right. */
function CardHeader({ className, ...props }: ComponentProps<'div'>) {
  return (
    <div data-slot="card-header" className={cn('flex items-center justify-between gap-2', className)} {...props} />
  );
}

function CardTitle({ className, ...props }: ComponentProps<'h2'>) {
  return (
    <h2
      data-slot="card-title"
      className={cn('m-0 flex items-center gap-2 text-[13px] font-medium text-muted-foreground', className)}
      {...props}
    />
  );
}

function CardMeta({ className, ...props }: ComponentProps<'span'>) {
  return <span data-slot="card-meta" className={cn('font-mono text-xs text-muted-foreground', className)} {...props} />;
}

export { Card, CardHeader, CardMeta, CardTitle };
