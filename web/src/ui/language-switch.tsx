import { LOCALE_NAMES, getLocale, locales, setLocale } from '@/lib/i18n';
import { m } from '@/paraglide/messages';
import { cn } from '@/lib/utils';

/** The language links in the footer; a change reloads the page in that language. */
export function LanguageSwitch() {
  const current = getLocale();
  return (
    <div role="group" aria-label={m.footer_language()} className="flex gap-1">
      {locales.map((l) => (
        <button
          key={l}
          type="button"
          lang={l}
          aria-pressed={l === current}
          onClick={() => setLocale(l)}
          className={cn(
            'cursor-pointer rounded-sm px-2 py-1 text-xs outline-none focus-visible:ring-2 focus-visible:ring-ring/60',
            l === current ? 'bg-secondary text-foreground' : 'text-muted-foreground hover:text-foreground',
          )}
        >
          {LOCALE_NAMES[l]}
        </button>
      ))}
    </div>
  );
}
