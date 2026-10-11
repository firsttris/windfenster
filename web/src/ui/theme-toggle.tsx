import { Moon, Sun } from 'lucide-react';
import { useTheme } from '@/lib/prefs';
import { m } from '@/paraglide/messages';
import { Button } from './button';

/** Switches between light and dark; without a choice the system decides. */
export function ThemeToggle() {
  const [theme, setTheme] = useTheme();
  const next = theme === 'dark' ? 'light' : 'dark';
  return (
    <Button
      variant="outline"
      size="icon"
      aria-label={next === 'light' ? m.theme_toLight() : m.theme_toDark()}
      title={next === 'light' ? m.theme_toLight() : m.theme_toDark()}
      onClick={() => setTheme(next)}
    >
      {theme === 'dark' ? <Sun aria-hidden="true" /> : <Moon aria-hidden="true" />}
    </Button>
  );
}
