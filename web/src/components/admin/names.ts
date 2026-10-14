/** The names of the server's tasks and sources, as the admin page shows them. */
import { m } from '@/paraglide/messages';

const TASKS: Record<string, () => string> = {
  runs: m.admin_taskRuns,
  refresh: m.admin_taskRefresh,
  metar: m.admin_taskMetar,
  taf: m.admin_taskTaf,
  dwd: m.admin_taskDwd,
  gauges: m.admin_taskGauges,
  stations: m.admin_taskStations,
  purge: m.admin_taskPurge,
  map: m.admin_taskMap,
  icon_global: m.admin_taskIconGlobal,
  basemap: m.admin_taskBasemap,
  spots: m.admin_taskSpots,
  alarms: m.admin_taskAlarms,
};

export function taskName(id: string): string {
  return TASKS[id]?.() ?? id;
}

/** Sources go by their own names in every language. */
const SOURCES: Record<string, string> = {
  open_meteo: 'Open-Meteo',
  awc: 'aviationweather.gov',
  dwd: 'DWD Open Data',
  ecmwf: 'ECMWF',
  pegelonline: 'PEGELONLINE',
  protomaps: 'Protomaps',
  mapterhorn: 'Mapterhorn',
  iem: 'Iowa Environmental Mesonet',
  ourairports: 'OurAirports',
};

export function sourceName(id: string): string {
  return SOURCES[id] ?? id;
}
