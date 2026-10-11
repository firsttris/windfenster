/**
 * Our own basemap holds the whole region to `fullZoom` and only boxes around the spots deeper.
 * MapLibre would show nothing where a deeper tile is missing, so the map reads the file as two
 * sources: `pm` to `fullZoom` everywhere (enlarged beyond it) and `pm-near` deeper inside the
 * boxes. Fills of both stack (the near ones cover the far ones where they have tiles); lines and
 * labels are split at the boxes' edges (`within`), so nothing is drawn twice.
 */
import type { ExpressionSpecification, LayerSpecification, SourceSpecification, StyleSpecification } from 'maplibre-gl';
import type { BasemapInfo } from '@/bindings/BasemapInfo';
import type { ThemePaint } from './palette';

const FAR = 'pm';
const NEAR = 'pm-near';
/** Layers under the wind field: the near ones go over the far ones. */
const FILLS = new Set(['earth', 'landuse', 'water', 'buildings']);
/** Over the field from the far source only: polygons, which `within` cannot split, and gone by
 * town zoom anyway (lakes over the field) or close enough (the coast line). */
const FAR_ONLY = new Set(['water-inland', 'coast']);

const near = (id: string) => `${id}-near`;

/** The boxes as one MultiPolygon, for `within`. */
export function boxes(detail: readonly (readonly [number, number, number, number])[]): ExpressionSpecification {
  return [
    'within',
    {
      type: 'MultiPolygon',
      coordinates: detail.map(([w, s, e, n]) => [
        [
          [w, s],
          [e, s],
          [e, n],
          [w, n],
          [w, s],
        ],
      ]),
    },
  ];
}

function andFilter(l: LayerSpecification, f: ExpressionSpecification): ExpressionSpecification {
  return 'filter' in l && l.filter ? (['all', l.filter, f] as ExpressionSpecification) : f;
}

/** The style's layers for the two sources, in drawing order. */
export function splitLayers(layers: LayerSpecification[], info: BasemapInfo): LayerSpecification[] {
  const inside = boxes(info.detail);
  const sourced = layers.filter((l) => 'source' in l);
  const fills = sourced.filter((l) => FILLS.has(l.id));
  const overlays = sourced.filter((l) => !FILLS.has(l.id));
  const split = overlays.filter((l) => !FAR_ONLY.has(l.id));
  return [
    ...layers.filter((l) => !('source' in l)),
    ...fills,
    ...fills.map((l) => ({ ...l, id: near(l.id), source: NEAR }) as LayerSpecification),
    ...overlays.map((l) =>
      FAR_ONLY.has(l.id) ? l : ({ ...l, filter: andFilter(l, ['!', inside]) } as LayerSpecification),
    ),
    ...split.map((l) => ({ ...l, id: near(l.id), source: NEAR, filter: andFilter(l, inside) }) as LayerSpecification),
  ];
}

/** The theme's paint for the near layers too */
export function splitPaint(paint: ThemePaint): ThemePaint {
  return { ...paint, ...Object.fromEntries(Object.entries(paint).map(([id, p]) => [near(id), p])) };
}

/** The file as two sources of the same tiles */
export function splitSources(info: BasemapInfo, attribution: string): StyleSpecification['sources'] {
  const tiles = [`pmtiles://${location.origin}/api/map/basemap.pmtiles/{z}/{x}/{y}`];
  const union: [number, number, number, number] = [
    Math.min(...info.detail.map((b) => b[0])),
    Math.min(...info.detail.map((b) => b[1])),
    Math.max(...info.detail.map((b) => b[2])),
    Math.max(...info.detail.map((b) => b[3])),
  ];
  const far: SourceSpecification = {
    type: 'vector',
    tiles,
    minzoom: info.minZoom,
    maxzoom: info.fullZoom,
    bounds: info.bounds as [number, number, number, number],
    attribution,
  };
  const close: SourceSpecification = {
    type: 'vector',
    tiles,
    minzoom: info.fullZoom + 1,
    maxzoom: info.maxZoom,
    bounds: union,
  };
  return { [FAR]: far, [NEAR]: close };
}
