/**
 * What each model is good for, for the models page: its area, where it is strong and what to
 * mind, in the viewer's language; and which models to look at first per area. The numbers
 * (resolution, reach, runs) come from the server's model list, not from here. Areas and reach as
 * in docs/forecasts.md and the providers' domains in the migrations.
 */
import type { Model } from '@/bindings/Model';
import { m } from '@/paraglide/messages';

export interface ModelGuide {
  /** None for a global model: it covers the world */
  area?: () => string;
  good: () => string;
  watch: () => string;
}

/** Open-Meteo's model id and its texts */
export const MODEL_GUIDE: Record<string, ModelGuide> = {
  icon_d2: { area: m.guide_iconD2_area, good: m.guide_iconD2_good, watch: m.guide_iconD2_watch },
  meteoswiss_icon_ch1: { area: m.guide_iconCh1_area, good: m.guide_iconCh1_good, watch: m.guide_iconCh1_watch },
  meteoswiss_icon_ch2: { area: m.guide_iconCh2_area, good: m.guide_iconCh2_good, watch: m.guide_iconCh2_watch },
  meteofrance_arome_france_hd: { area: m.guide_arome_area, good: m.guide_arome_good, watch: m.guide_arome_watch },
  dmi_harmonie_arome_europe: {
    area: m.guide_harmonie_area,
    good: m.guide_harmonie_good,
    watch: m.guide_harmonie_watch,
  },
  italia_meteo_arpae_icon_2i: { area: m.guide_icon2i_area, good: m.guide_icon2i_good, watch: m.guide_icon2i_watch },
  ecmwf_ifs025: { good: m.guide_ecmwf_good, watch: m.guide_ecmwf_watch },
  ukmo_global_deterministic_10km: { good: m.guide_ukmo_good, watch: m.guide_ukmo_watch },
  icon_eu: { area: m.guide_iconEu_area, good: m.guide_iconEu_good, watch: m.guide_iconEu_watch },
  gfs_global: { good: m.guide_gfs_good, watch: m.guide_gfs_watch },
};

export interface Region {
  name: () => string;
  /** The models to look at first for today and tomorrow, best first */
  soon: readonly string[];
  /** And for the days after */
  later: readonly string[];
}

export const REGIONS: readonly Region[] = [
  { name: m.guide_rNorth, soon: ['icon_d2', 'dmi_harmonie_arome_europe'], later: ['ecmwf_ifs025', 'icon_eu'] },
  { name: m.guide_rDenmark, soon: ['dmi_harmonie_arome_europe', 'icon_d2'], later: ['ecmwf_ifs025', 'icon_eu'] },
  {
    name: m.guide_rAlps,
    soon: ['meteoswiss_icon_ch1', 'icon_d2'],
    later: ['meteoswiss_icon_ch2', 'ecmwf_ifs025'],
  },
  { name: m.guide_rItaly, soon: ['italia_meteo_arpae_icon_2i'], later: ['italia_meteo_arpae_icon_2i', 'ecmwf_ifs025'] },
  { name: m.guide_rFrance, soon: ['meteofrance_arome_france_hd'], later: ['ecmwf_ifs025', 'icon_eu'] },
  { name: m.guide_rMed, soon: ['icon_eu'], later: ['ecmwf_ifs025', 'icon_eu'] },
  {
    name: m.guide_rWorld,
    soon: ['ecmwf_ifs025', 'ukmo_global_deterministic_10km'],
    later: ['ecmwf_ifs025', 'gfs_global'],
  },
];

/** The models of `ids` this server runs (`WF_MODELS` or the admin page can switch some off), in
 * their order. */
export function running(ids: readonly string[], models: readonly Model[]): Model[] {
  return ids.flatMap((id) => models.filter((x) => x.id === id));
}

/** The server's models in the page's order: the fine ones first, each class by its rank. */
export function inOrder(models: readonly Model[]): Model[] {
  const cls = (x: Model) => (x.class === 'super' ? 0 : 1);
  return [...models].sort((a, b) => cls(a) - cls(b) || a.rank - b.rank);
}
