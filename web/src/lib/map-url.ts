/** What the map shows beyond the view (which MapLibre keeps in the hash): the hour, the layer
 * and the model, in the query, so a shared link opens the same picture. A forecast page's
 * "show on map" adds the place to open (`spot`, `station`), read once. */

export interface MapState {
  /** Unix seconds; absent: the hour nearest to now */
  time: number | null;
  gusts: boolean;
  /** Model id; absent: the finest with hours */
  model: string | null;
}

/** The place a link opens the map on */
export interface MapSpot {
  lat: number;
  lon: number;
  slug: string | null;
}

/** Zoom a "show on map" link opens: town zoom, the stations as badges */
const SPOT_ZOOM = 12;

/** The map at a place, with the place picked */
export function mapHref({ lat, lon, slug }: MapSpot): string {
  const q = new URLSearchParams({ spot: `${lat.toFixed(4)},${lon.toFixed(4)}` });
  if (slug) q.set('station', slug);
  return `/map?${q}#${SPOT_ZOOM}/${lat.toFixed(4)}/${lon.toFixed(4)}`;
}

/** The place in a map link, or null */
export function parseSpot(search: string): MapSpot | null {
  const q = new URLSearchParams(search);
  const [lat, lon] = (q.get('spot') ?? '').split(',').map(Number);
  if (!Number.isFinite(lat) || !Number.isFinite(lon) || Math.abs(lat) > 90 || Math.abs(lon) > 180) return null;
  const slug = q.get('station');
  return { lat, lon, slug: slug && /^[a-z0-9-]{1,40}$/.test(slug) ? slug : null };
}

export function parseMapState(search: string): Partial<MapState> {
  const q = new URLSearchParams(search);
  const out: Partial<MapState> = {};
  const t = Number(q.get('t'));
  if (q.has('t') && Number.isFinite(t) && t > 0) out.time = Math.round(t / 3600) * 3600;
  if (q.has('layer')) out.gusts = q.get('layer') === 'gusts';
  const model = q.get('model');
  if (model && /^[a-z0-9_]{1,40}$/.test(model)) out.model = model;
  return out;
}

/** The query for a state, keeping parameters that are not the map's. */
export function mapQuery(current: string, s: MapState): string {
  const q = new URLSearchParams(current);
  const set = (k: string, v: string | null) => (v === null ? q.delete(k) : q.set(k, v));
  set('t', s.time === null ? null : String(s.time));
  set('layer', s.gusts ? 'gusts' : null);
  set('model', s.model);
  // The place a link opened is shown once; it does not stay in the address.
  set('spot', null);
  set('station', null);
  const out = q.toString();
  return out ? `?${out}` : '';
}
