/** The basemap's colours, light and dark: calm, so the wind field leads; lines and labels drawn
 * over the field are translucent so they read on any wind colour. */
import type { Map as MapLibreMap } from 'maplibre-gl';
import type { Theme } from '@/lib/prefs';

export interface Palette {
  sea: string;
  land: string;
  green: string;
  line: string;
  coast: string;
  border: string;
  region: string;
  river: string;
  highway: string;
  major: string;
  minor: string;
  text: string;
  halo: string;
  water: string;
}

export const PALETTES: Record<Theme, Palette> = {
  dark: {
    sea: '#0b0d12',
    land: '#1b1c21',
    green: '#1a201c',
    line: 'rgba(250,250,250,0.55)',
    coast: 'rgba(250,250,250,0.62)',
    border: 'rgba(250,250,250,0.42)',
    region: 'rgba(250,250,250,0.18)',
    river: 'rgba(160,190,230,0.22)',
    highway: 'rgba(250,250,250,0.42)',
    major: 'rgba(250,250,250,0.28)',
    minor: 'rgba(250,250,250,0.15)',
    text: '#f4f4f5',
    halo: 'rgba(9,9,11,0.78)',
    water: '#9db7d8',
  },
  light: {
    sea: '#d7e0ea',
    land: '#f4f4f5',
    green: '#e9efe7',
    line: 'rgba(24,24,27,0.6)',
    coast: 'rgba(24,24,27,0.55)',
    border: 'rgba(24,24,27,0.4)',
    region: 'rgba(24,24,27,0.18)',
    river: 'rgba(40,90,150,0.25)',
    highway: 'rgba(24,24,27,0.42)',
    major: 'rgba(24,24,27,0.28)',
    minor: 'rgba(24,24,27,0.14)',
    text: '#18181b',
    halo: 'rgba(255,255,255,0.85)',
    water: '#3f6a9a',
  },
};

export type PaintProperty = Parameters<MapLibreMap['setPaintProperty']>[1];

/** Paint properties that change with the theme, per layer id */
export type ThemePaint = Record<string, Record<string, string>>;
