/**
 * The colour of a model's dot and line. It follows the model, not its place in the list, so a
 * model keeps its colour from station to station; the mix and the TAF have their own. Ten
 * slots (--chart-1 to --chart-10 in styles.css), apart from each other, the mix and the
 * measured values by at least ΔE 9 (OKLab ×100) in light and dark; the name always stands
 * beside the colour.
 */
import type { Forecast } from '@/bindings/Forecast';
import { MIX } from './forecast';

const SLOTS = 10;

/** Open-Meteo's model id and its slot */
export const SLOT: Record<string, number> = {
  icon_d2: 1,
  meteofrance_arome_france_hd: 2,
  dmi_harmonie_arome_europe: 3,
  ecmwf_ifs025: 4,
  gfs_global: 5,
  icon_eu: 6,
  italia_meteo_arpae_icon_2i: 7,
  ukmo_global_deterministic_10km: 8,
  meteoswiss_icon_ch1: 9,
  meteoswiss_icon_ch2: 10,
};

export function modelColor(fc: Pick<Forecast, 'models'>, id: string): string {
  if (id === MIX) return 'var(--chart-mix)';
  if (id === 'taf') return 'var(--success-foreground)';
  // A model without a slot of its own takes the first slot no model on the page holds.
  const slot =
    SLOT[id] ??
    (() => {
      const taken = new Set(fc.models.map((x) => SLOT[x.id]));
      const unknown = fc.models.filter((x) => x.id !== MIX && SLOT[x.id] === undefined).map((x) => x.id);
      const free = Array.from({ length: SLOTS }, (_, i) => i + 1).filter((s) => !taken.has(s));
      return free.length ? free[Math.max(0, unknown.indexOf(id)) % free.length] : 1;
    })();
  return `var(--chart-${slot})`;
}
