/**
 * Every outside source Windfenster shows data from, with its licence and the credit it asks
 * for, for the sources page. Licences as the providers state them: Open-Meteo's for itself and
 * the models it serves (open-meteo.com/en/licence), the DWD's on dwd.de/copyright; PEGELONLINE
 * links its own terms.
 */
import { m } from '@/paraglide/messages';

export interface Licence {
  /** A licence's own name, or words in the viewer's language for a provider's terms */
  name: string | (() => string);
  url: string;
}

export interface Source {
  id: string;
  name: string;
  url: string;
  /** What Windfenster takes from it, in the viewer's language */
  use: () => string;
  licence: Licence;
  /** The credit as the provider asks for it, where it asks for a wording */
  credit?: string;
}

const CC_BY = { name: 'CC BY 4.0', url: 'https://creativecommons.org/licenses/by/4.0/' };
const PUBLIC_DOMAIN_US = { name: 'Public Domain (U.S. Government)', url: 'https://www.usa.gov/government-copyright' };

/** Weather data, measurements and the map's data */
export const DATA_SOURCES: readonly Source[] = [
  {
    id: 'open-meteo',
    name: 'Open-Meteo',
    url: 'https://open-meteo.com/',
    use: m.sources_openMeteo,
    licence: { ...CC_BY, url: 'https://open-meteo.com/en/licence' },
    credit: 'Weather data by Open-Meteo.com',
  },
  {
    id: 'dwd',
    name: 'Deutscher Wetterdienst (DWD)',
    url: 'https://opendata.dwd.de/',
    use: m.sources_dwd,
    // dwd.de/copyright: open geodata under CC BY 4.0 with a source note
    licence: { ...CC_BY, url: 'https://www.dwd.de/copyright' },
    credit: 'Datenbasis: Deutscher Wetterdienst, eigene Berechnungen',
  },
  {
    id: 'ecmwf',
    name: 'ECMWF Open Data',
    url: 'https://www.ecmwf.int/en/forecasts/datasets/open-data',
    use: m.sources_ecmwf,
    licence: CC_BY,
    credit: '© ECMWF',
  },
  {
    id: 'meteofrance',
    name: 'Météo-France (AROME)',
    url: 'https://meteofrance.fr/',
    use: m.sources_meteoFrance,
    licence: {
      name: 'Licence Ouverte (Etalab)',
      url: 'https://donneespubliques.meteofrance.fr/?fond=dossier&id_dossier=1',
    },
  },
  {
    id: 'dmi',
    name: 'Danmarks Meteorologiske Institut (HARMONIE)',
    url: 'https://www.dmi.dk/',
    use: m.sources_dmi,
    licence: CC_BY,
  },
  {
    id: 'italiameteo',
    name: 'ItaliaMeteo-ARPAE (ICON-2I)',
    url: 'https://www.agenziaitaliameteo.it/',
    use: m.sources_italiaMeteo,
    licence: CC_BY,
  },
  {
    id: 'meteoswiss',
    name: 'MeteoSwiss (ICON-CH1, ICON-CH2)',
    url: 'https://www.meteoswiss.admin.ch/',
    use: m.sources_meteoSwiss,
    licence: CC_BY,
  },
  {
    id: 'metoffice',
    name: 'Met Office (UKMO Global)',
    url: 'https://www.metoffice.gov.uk/',
    use: m.sources_metOffice,
    // open-meteo.com/en/licence: the Met Office's data under CC BY-SA 4.0
    licence: { name: 'CC BY-SA 4.0', url: 'https://creativecommons.org/licenses/by-sa/4.0/' },
  },
  {
    id: 'noaa-gfs',
    name: 'NOAA NCEP (GFS)',
    url: 'https://www.weather.gov/ncep/',
    use: m.sources_gfs,
    licence: PUBLIC_DOMAIN_US,
  },
  {
    id: 'awc',
    name: 'Aviation Weather Center (NOAA)',
    url: 'https://aviationweather.gov/',
    use: m.sources_awc,
    licence: PUBLIC_DOMAIN_US,
  },
  {
    id: 'iem',
    name: 'Iowa Environmental Mesonet',
    url: 'https://mesonet.agron.iastate.edu/',
    use: m.sources_iem,
    // mesonet.agron.iastate.edu/disclaimer.php: public domain, a credit appreciated
    licence: { name: 'Public Domain', url: 'https://mesonet.agron.iastate.edu/disclaimer.php' },
    credit: 'Iowa Environmental Mesonet of Iowa State University',
  },
  {
    id: 'pegelonline',
    name: 'PEGELONLINE (WSV)',
    url: 'https://www.pegelonline.wsv.de/',
    use: m.sources_pegelonline,
    licence: { name: m.sources_termsOf, url: 'https://www.pegelonline.wsv.de/gast/nutzungsbedingungen' },
    credit: 'Wasserstraßen- und Schifffahrtsverwaltung des Bundes (WSV)',
  },
  {
    id: 'ourairports',
    name: 'OurAirports',
    url: 'https://ourairports.com/data/',
    use: m.sources_ourAirports,
    licence: { name: 'Public Domain', url: 'https://ourairports.com/data/' },
  },
  {
    id: 'geonames',
    name: 'GeoNames',
    url: 'https://www.geonames.org/',
    use: m.sources_geonames,
    licence: CC_BY,
  },
  {
    id: 'osm',
    name: 'OpenStreetMap',
    url: 'https://www.openstreetmap.org/',
    use: m.sources_osm,
    licence: { name: 'ODbL 1.0', url: 'https://www.openstreetmap.org/copyright' },
    credit: '© OpenStreetMap contributors',
  },
  {
    id: 'mapterhorn',
    name: 'Mapterhorn',
    url: 'https://mapterhorn.com/',
    use: m.sources_mapterhorn,
    licence: { name: m.sources_attributionList, url: 'https://mapterhorn.com/attribution' },
    credit: '© Mapterhorn',
  },
  {
    id: 'natural-earth',
    name: 'Natural Earth',
    url: 'https://www.naturalearthdata.com/',
    use: m.sources_naturalEarth,
    licence: { name: 'Public Domain', url: 'https://www.naturalearthdata.com/about/terms-of-use/' },
  },
];

/** Software and fonts the pages are made with, beyond the code's own dependencies */
export const SOFTWARE: readonly Source[] = [
  {
    id: 'maplibre',
    name: 'MapLibre GL JS',
    url: 'https://maplibre.org/',
    use: m.sources_maplibre,
    licence: { name: 'BSD-3-Clause', url: 'https://github.com/maplibre/maplibre-gl-js/blob/main/LICENSE.txt' },
  },
  {
    id: 'protomaps',
    name: 'Protomaps',
    url: 'https://protomaps.com/',
    use: m.sources_protomaps,
    licence: { name: 'BSD-3-Clause', url: 'https://github.com/protomaps/PMTiles/blob/main/LICENSE' },
  },
  {
    id: 'noto',
    name: 'Noto Sans',
    url: 'https://fonts.google.com/noto/specimen/Noto+Sans',
    use: m.sources_noto,
    licence: { name: 'SIL OFL 1.1', url: '/glyphs/OFL.txt' },
  },
];
