import { cva, type VariantProps } from 'class-variance-authority';
import type { ComponentProps } from 'react';
import { cn } from '@/lib/utils';

const badgeVariants = cva('inline-flex items-center gap-1.5 whitespace-nowrap rounded-full px-2.5 py-0.5 text-xs', {
  variants: {
    variant: {
      outline: 'border text-subtle',
      secondary: 'border bg-secondary text-subtle',
      success: 'border border-success-border bg-success-surface text-success-foreground',
      solid: 'bg-primary font-semibold text-primary-foreground',
    },
  },
  defaultVariants: { variant: 'outline' },
});

function Badge({ className, variant, ...props }: ComponentProps<'span'> & VariantProps<typeof badgeVariants>) {
  return <span data-slot="badge" className={cn(badgeVariants({ variant }), className)} {...props} />;
}

export { Badge, badgeVariants };
