/** From afar the map is the wind; from town zoom on it is a map again: the colour field fades out
 * between these zooms and the basemap's land use, water and roads come forward, while the
 * particles take the field's colours so the strength stays readable. */

/** MapLibre zooms: the field starts to fade, and is gone */
export const TOWN_FROM = 9.5;
export const TOWN_TO = 11.5;

/** MapLibre's zoom of a view `mercatorWidth` wide (0…1) on `cssWidth` pixels: the world is 512
 * CSS pixels wide at zoom 0. */
export function zoomOf(mercatorWidth: number, cssWidth: number): number {
  return Math.log2(cssWidth / (mercatorWidth * 512));
}

/** How far into town zoom: 0 up to TOWN_FROM, 1 from TOWN_TO, smooth between. */
export function townness(zoom: number): number {
  const t = Math.min(1, Math.max(0, (zoom - TOWN_FROM) / (TOWN_TO - TOWN_FROM)));
  return t * t * (3 - 2 * t);
}
