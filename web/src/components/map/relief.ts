/**
 * Relief under the wind, as Windy and Windfinder have it: hillshade from Mapterhorn's elevation
 * (Terrarium-encoded WebP in a PMTiles file the server cut for the region). It lies over the
 * colour field, so mountains and valleys show through the wind's colours, and under water,
 * roads and labels; softer from town zoom on, where the map itself leads.
 */
import type { ExpressionSpecification, LayerSpecification } from 'maplibre-gl';
import type { Theme } from '@/lib/prefs';
import type { Basemap } from './basemap';

export const MAPTERHORN_ATTRIBUTION = '<a href="https://mapterhorn.com/attribution">© Mapterhorn</a>';

const strength: ExpressionSpecification = ['interpolate', ['linear'], ['zoom'], 4, 0.3, 8, 0.5, 11, 0.45, 14, 0.25];

function reliefPaint(theme: Theme): Record<string, string | ExpressionSpecification> {
  return theme === 'dark'
    ? {
        'hillshade-shadow-color': 'rgba(0,0,0,0.55)',
        'hillshade-highlight-color': 'rgba(255,255,255,0.12)',
        'hillshade-accent-color': 'rgba(0,0,0,0.25)',
        'hillshade-exaggeration': strength,
      }
    : {
        'hillshade-shadow-color': 'rgba(24,32,44,0.38)',
        'hillshade-highlight-color': 'rgba(255,255,255,0.32)',
        'hillshade-accent-color': 'rgba(24,32,44,0.18)',
        'hillshade-exaggeration': strength,
      };
}

/** The basemap with relief; the wind field goes under it. */
export function withRelief(base: Basemap): Basemap {
  return {
    ...base,
    style: (theme, lang) => {
      const s = base.style(theme, lang);
      const hillshade = {
        id: 'hillshade',
        type: 'hillshade',
        source: 'terrain',
        paint: reliefPaint(theme),
      } as LayerSpecification;
      const at = s.layers.findIndex((l) => l.id === base.aboveField);
      const layers = [...s.layers];
      layers.splice(at < 0 ? layers.length : at, 0, hillshade);
      return {
        ...s,
        sources: {
          ...s.sources,
          terrain: {
            type: 'raster-dem',
            url: `pmtiles://${location.origin}/api/map/terrain.pmtiles`,
            encoding: 'terrarium',
            tileSize: 512,
            attribution: MAPTERHORN_ATTRIBUTION,
          },
        },
        layers,
      };
    },
    aboveField: 'hillshade',
    paint: (theme) => ({ ...base.paint(theme), hillshade: reliefPaint(theme) }),
  };
}
