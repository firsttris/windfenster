/**
 * Our own basemap holds the whole region to `fullZoom`; deeper only boxes around the spots
 * people look at, served by tile (`/api/map/basemap/{z}/{x}/{y}`, 404 elsewhere). MapLibre would
 * show nothing where a deeper tile is missing, so the map has two sources: `pm`, the region's
 * file to `fullZoom` everywhere (enlarged beyond it), and `pm-near`, the detail tiles. Fills of
 * both stack (the near ones cover the far ones where they have tiles); lines and labels are
 * split at the boxes' edges (`within`), so nothing is drawn twice. New boxes only change those
 * filters (`splitFilters`), so they show without making the map anew.
 */
import type {
  ExpressionSpecification,
  FilterSpecification,
  LayerSpecification,
  SourceSpecification,
  StyleSpecification,
} from 'maplibre-gl';
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

type Box = readonly [number, number, number, number];
/** Stands in while there are no boxes: a speck in the Gulf of Guinea holds nothing. */
const NOWHERE: Box = [0, 0, 0.0001, 0.0001];

/** The boxes as one MultiPolygon, for `within`. */
export function boxes(detail: readonly Box[]): ExpressionSpecification {
  return [
    'within',
    {
      type: 'MultiPolygon',
      coordinates: (detail.length ? detail : [NOWHERE]).map(([w, s, e, n]) => [
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

/** Whether the file has detail beyond the region, so the map needs both sources */
export function hasDetail(info: BasemapInfo): boolean {
  return info.detailZoom > info.fullZoom;
}

/** The style's layers for the two sources, in drawing order. */
export function splitLayers(layers: LayerSpecification[], detail: readonly Box[]): LayerSpecification[] {
  const filters = new Map(splitFilters(layers, detail));
  const sourced = layers.filter((l) => 'source' in l);
  const fills = sourced.filter((l) => FILLS.has(l.id));
  const overlays = sourced.filter((l) => !FILLS.has(l.id));
  const split = overlays.filter((l) => !FAR_ONLY.has(l.id));
  const withFilter = (l: LayerSpecification, id: string) =>
    ({ ...l, id, filter: filters.get(id) }) as LayerSpecification;
  return [
    ...layers.filter((l) => !('source' in l)),
    ...fills,
    ...fills.map((l) => ({ ...l, id: near(l.id), source: NEAR }) as LayerSpecification),
    ...overlays.map((l) => (FAR_ONLY.has(l.id) ? l : withFilter(l, l.id))),
    ...split.map((l) => ({ ...withFilter(l, near(l.id)), source: NEAR }) as LayerSpecification),
  ];
}

/** The filters that split lines and labels at the boxes: far ones outside, near ones inside. */
export function splitFilters(
  layers: readonly LayerSpecification[],
  detail: readonly Box[],
): [string, FilterSpecification][] {
  const inside = boxes(detail);
  return layers
    .filter((l) => 'source' in l && !FILLS.has(l.id) && !FAR_ONLY.has(l.id))
    .flatMap((l) => [
      [l.id, andFilter(l, ['!', inside])],
      [near(l.id), andFilter(l, inside)],
    ]);
}

/** The theme's paint for the near layers too */
export function splitPaint(paint: ThemePaint): ThemePaint {
  return { ...paint, ...Object.fromEntries(Object.entries(paint).map(([id, p]) => [near(id), p])) };
}

/** The region's file and the detail tiles */
export function splitSources(info: BasemapInfo, attribution: string): StyleSpecification['sources'] {
  const bounds = info.bounds as [number, number, number, number];
  const far: SourceSpecification = {
    type: 'vector',
    tiles: [`pmtiles://${location.origin}/api/map/basemap.pmtiles/{z}/{x}/{y}`],
    minzoom: info.minZoom,
    maxzoom: info.fullZoom,
    bounds,
    attribution,
  };
  const close: SourceSpecification = {
    type: 'vector',
    tiles: [`${location.origin}/api/map/basemap/{z}/{x}/{y}`],
    minzoom: info.fullZoom + 1,
    maxzoom: info.detailZoom,
    bounds,
  };
  return { [FAR]: far, [NEAR]: close };
}
