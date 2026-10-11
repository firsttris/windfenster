import type { Weather } from '@/lib/forecast';
import { cn } from '@/lib/utils';

/** Sun, sun with cloud, cloud, rain: the mockup's symbols, coloured by the theme. */
export function WeatherIcon({ weather, label, className }: { weather: Weather; label: string; className?: string }) {
  const common = {
    viewBox: '0 0 24 24',
    fill: 'none',
    strokeWidth: 2,
    strokeLinecap: 'round' as const,
    strokeLinejoin: 'round' as const,
    role: 'img',
    className: cn('size-[22px]', className),
  };
  switch (weather) {
    case 'sun':
      return (
        <svg {...common} className={cn(common.className, 'stroke-sun')}>
          <title>{label}</title>
          <circle cx="12" cy="12" r="4" />
          <path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4" />
        </svg>
      );
    case 'partly':
      return (
        <svg {...common}>
          <title>{label}</title>
          <circle cx="8.5" cy="8" r="3" className="stroke-sun" />
          <path d="M8.5 2.5v1M3 8h1M4.6 4.1l.7.7" className="stroke-sun" />
          <path d="M17.5 20H9a4 4 0 1 1 .9-7.9A5 5 0 0 1 19 14a3 3 0 0 1-1.5 6z" className="stroke-subtle" />
        </svg>
      );
    case 'cloud':
      return (
        <svg {...common} className={cn(common.className, 'stroke-muted-foreground')}>
          <title>{label}</title>
          <path d="M17.5 19H8a5 5 0 1 1 1-9.9A6 6 0 0 1 20 11.5a3.5 3.5 0 0 1-2.5 7.5z" />
        </svg>
      );
    case 'rain':
      return (
        <svg {...common}>
          <title>{label}</title>
          <path d="M17 14H8a4 4 0 1 1 .8-7.9A5 5 0 0 1 18 8a3 3 0 0 1-1 6z" className="stroke-muted-foreground" />
          <path d="M8 18l-1 3M12 18l-1 3M16 18l-1 3" className="stroke-water" />
        </svg>
      );
  }
}
