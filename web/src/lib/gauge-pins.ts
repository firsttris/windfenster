/** The water gauges on the map: what each pin shows. */
import type { GaugePin } from '@/bindings/GaugePin';
import type { GaugeState } from '@/bindings/GaugeState';
import { formatNumber } from './format';

/** From this zoom on the map shows the gauges, with the stations' badges */
export const GAUGE_ZOOM = 8.5;

/** The pin's colour by the level against PEGELONLINE's marks: water blue as usual, amber low,
 * red high, grey without a fresh reading */
export const GAUGE_COLOR: Record<GaugeState, string> = {
  normal: '#38bdf8',
  low: '#f59e0b',
  high: '#ef4444',
  unknown: '#71717a',
};

export interface GaugeFeatureProps {
  uuid: string;
  name: string;
  color: string;
  /** "312 cm", empty without a fresh reading */
  label: string;
  /** "Iffezheim · Rhein" */
  title: string;
}

export function gaugeProps(g: GaugePin): GaugeFeatureProps {
  return {
    uuid: g.uuid,
    name: g.name,
    color: GAUGE_COLOR[g.state],
    label: g.levelCm !== null ? `${formatNumber(Math.round(g.levelCm))} cm` : '',
    title: g.water ? `${g.name} · ${g.water}` : g.name,
  };
}
