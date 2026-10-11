/**
 * Viewer preferences kept in this browser (localStorage): wind unit and theme. Storage can be
 * blocked (private windows); then the defaults apply and choices last until the page reloads.
 */
import { useSyncExternalStore } from 'react';
import type { WindUnit } from '@/bindings/WindUnit';
import { WIND_UNITS } from './units';

export type Theme = 'light' | 'dark';

const listeners = new Set<() => void>();
const memory = new Map<string, string>();

function read(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return memory.get(key) ?? null;
  }
}

function write(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    memory.set(key, value);
  }
  for (const l of listeners) l();
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

const UNIT_KEY = 'wf.unit';
const THEME_KEY = 'wf.theme';

const readUnit = (): WindUnit => {
  const v = read(UNIT_KEY);
  return (WIND_UNITS as readonly string[]).includes(v ?? '') ? (v as WindUnit) : 'kn';
};

export function useUnit(): [WindUnit, (u: WindUnit) => void] {
  const unit = useSyncExternalStore(subscribe, readUnit, () => 'kn' as const);
  return [unit, (u) => write(UNIT_KEY, u)];
}

const systemDark = () => typeof matchMedia === 'function' && matchMedia('(prefers-color-scheme: dark)').matches;

/** The theme in use: the one picked, else the system's. */
export function currentTheme(): Theme {
  const v = read(THEME_KEY);
  return v === 'light' || v === 'dark' ? v : systemDark() ? 'dark' : 'light';
}

function applyTheme() {
  document.documentElement.classList.toggle('dark', currentTheme() === 'dark');
}

/** Keeps the page's class in step with the choice and, without one, with the system. */
export function watchTheme(): () => void {
  applyTheme();
  const media = matchMedia('(prefers-color-scheme: dark)');
  const onSystem = () => {
    applyTheme();
    for (const l of listeners) l();
  };
  media.addEventListener('change', onSystem);
  const unsubscribe = subscribe(applyTheme);
  return () => {
    media.removeEventListener('change', onSystem);
    unsubscribe();
  };
}

export function useTheme(): [Theme, (t: Theme) => void] {
  const theme = useSyncExternalStore(subscribe, currentTheme, () => 'dark' as const);
  return [theme, (t) => write(THEME_KEY, t)];
}
