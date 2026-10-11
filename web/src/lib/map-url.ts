/** What the map shows beyond the view (which MapLibre keeps in the hash): the hour, the layer
 * and the model, in the query, so a shared link opens the same picture. */

export interface MapState {
  /** Unix seconds; absent: the hour nearest to now */
  time: number | null;
  gusts: boolean;
  /** Model id; absent: the finest with hours */
  model: string | null;
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
  const out = q.toString();
  return out ? `?${out}` : '';
}
