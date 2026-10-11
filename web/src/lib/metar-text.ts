/** A decoded METAR group in words, in the viewer's language and wind unit. */
import type { WindUnit } from '@/bindings/WindUnit';
import { m } from '@/paraglide/messages';
import { clock, compass, dayMonthOf, formatNumber, hour } from './format';
import type { Descriptor, Group, Phenomenon } from './metar';
import { formatWind, UNIT_LABEL } from './units';

export interface Context {
  unit: WindUnit;
  /** Unix seconds of the report */
  at: number;
  /** The station's time zone, for the local time */
  timeZone: string;
}

const DESCRIPTOR: Record<Descriptor, () => string> = {
  MI: m.wx_MI,
  BC: m.wx_BC,
  PR: m.wx_PR,
  DR: m.wx_DR,
  BL: m.wx_BL,
  SH: m.wx_SH,
  TS: m.wx_TS,
  FZ: m.wx_FZ,
};

const PHENOMENON: Record<Phenomenon, () => string> = {
  DZ: m.wx_DZ,
  RA: m.wx_RA,
  SN: m.wx_SN,
  SG: m.wx_SG,
  IC: m.wx_IC,
  PL: m.wx_PL,
  GR: m.wx_GR,
  GS: m.wx_GS,
  UP: m.wx_UP,
  BR: m.wx_BR,
  FG: m.wx_FG,
  FU: m.wx_FU,
  VA: m.wx_VA,
  DU: m.wx_DU,
  SA: m.wx_SA,
  HZ: m.wx_HZ,
  PO: m.wx_PO,
  SQ: m.wx_SQ,
  FC: m.wx_FC,
  SS: m.wx_SS,
  DS: m.wx_DS,
};

const COVER = { FEW: m.metar_cloudsFEW, SCT: m.metar_cloudsSCT, BKN: m.metar_cloudsBKN, OVC: m.metar_cloudsOVC };
const SKY = { NSC: m.metar_NSC, NCD: m.metar_NCD, SKC: m.metar_SKC, CLR: m.metar_SKC, NSW: m.metar_NSW };
const TREND = { NOSIG: m.metar_NOSIG, BECMG: m.metar_BECMG, TEMPO: m.metar_TEMPO };
const TREND_TIME = { FM: m.metar_FM, TL: m.metar_TL, AT: m.metar_AT };
const INTENSITY = { light: m.wx_light, heavy: m.wx_heavy, vicinity: m.wx_vicinity };

const pad = (n: number) => String(n).padStart(2, '0');
const speed = (kn: number, unit: WindUnit) => `${formatWind(kn, unit)} ${UNIT_LABEL[unit]}`;
const meters = (n: number) => (n >= 5000 ? `${formatNumber(n / 1000)} km` : `${formatNumber(n)} m`);

/** "Regen (Schauer, leicht)": what falls or hangs in the air, then how. */
function weather(g: Extract<Group, { kind: 'weather' }>): string {
  const descriptor = g.descriptor ? DESCRIPTOR[g.descriptor as Descriptor]() : null;
  const what = g.phenomena.map((p) => PHENOMENON[p as Phenomenon]?.() ?? p);
  const how = [
    what.length ? descriptor : null,
    g.intensity ? INTENSITY[g.intensity]() : null,
    g.recent ? m.wx_recent() : null,
  ].filter((x): x is string => !!x);
  const main = what.length ? what.join(', ') : (descriptor ?? g.token);
  return how.length ? `${main} (${how.join(', ')})` : main;
}

export function describe(g: Group, ctx: Context): string {
  switch (g.kind) {
    case 'time':
      return m.metar_time({
        date: dayMonthOf(ctx.at, 'UTC'),
        utc: `${pad(g.hour)}:${pad(g.minute)}`,
        local: clock(ctx.at, ctx.timeZone),
      });
    case 'auto':
      return m.metar_auto();
    case 'cor':
      return m.metar_cor();
    case 'nil':
      return m.metar_nil();
    case 'wind': {
      if (g.kn === 0) return m.metar_calm();
      const wind =
        g.dir === null
          ? m.metar_windVariable({ speed: speed(g.kn, ctx.unit) })
          : m.metar_wind({ deg: g.dir, compass: compass(g.dir), speed: speed(g.kn, ctx.unit) });
      return g.gustKn === null ? wind : m.metar_gusts({ wind, gust: speed(g.gustKn, ctx.unit) });
    }
    case 'windVariation':
      return m.metar_windVariation({ from: g.from, to: g.to });
    case 'cavok':
      return m.metar_cavok();
    case 'visibility':
      if (g.atLeast) return m.metar_visibilityMax();
      if (g.below) return m.metar_visibilityMin();
      return m.metar_visibility({ distance: meters(g.meters) });
    case 'visibilityMiles': {
      const args = { miles: formatNumber(g.miles, g.miles % 1 ? 1 : 0), distance: meters(g.miles * 1609) };
      return g.atLeast ? m.metar_visibilityMilesMore(args) : m.metar_visibilityMiles(args);
    }
    case 'rvr':
      return m.metar_rvr({ runway: g.runway });
    case 'runwayState':
      return m.metar_runwayState();
    case 'windShear':
      return m.metar_windShear();
    case 'weather':
      return weather(g);
    case 'notObserved':
      return m.metar_notObserved();
    case 'clouds': {
      if (g.feet === null) return m.metar_notObserved();
      const height = formatNumber(g.feet);
      const clouds = g.cover ? COVER[g.cover]({ height }) : m.metar_cloudsUnknown({ height });
      if (g.type === 'CB') return m.metar_cloudsCB({ clouds });
      if (g.type === 'TCU') return m.metar_cloudsTCU({ clouds });
      return clouds;
    }
    case 'verticalVisibility':
      return g.feet === null ? m.metar_skyObscured() : m.metar_verticalVisibility({ height: formatNumber(g.feet) });
    case 'sky':
      return SKY[g.code]();
    case 'temperature':
      return g.dew === null
        ? m.metar_temperatureOnly({ temp: g.temp })
        : m.metar_temperature({ temp: g.temp, dew: g.dew });
    case 'pressure':
      return m.metar_pressure({ hpa: g.hpa });
    case 'sea': {
      const sea = m.metar_sea({ temp: g.temp ?? '–' });
      if (g.waveM !== null) return m.metar_seaWaves({ sea, height: formatNumber(g.waveM, 1) });
      if (g.state !== null) return m.metar_seaState({ sea, state: g.state });
      return sea;
    }
    case 'colour':
      return m.metar_colour();
    case 'trend':
      return TREND[g.code]();
    case 'trendTime':
      return TREND_TIME[g.prefix]({ time: `${hour(g.hour)}:${pad(g.minute)}` });
    case 'remarks':
      return m.metar_remarks();
    case 'unknown':
      return m.metar_unknown();
  }
}

/** The groups shown as chips next to the measured wind: temperature, pressure, visibility and
 * the lowest clouds, in that order, before any trend. */
export function summary(groups: Group[]): Group[] {
  const end = groups.findIndex((g) => g.kind === 'trend' || g.kind === 'remarks');
  const now = end < 0 ? groups : groups.slice(0, end);
  const first = (kinds: Group['kind'][]) => now.find((g) => kinds.includes(g.kind));
  return [
    first(['temperature']),
    first(['pressure']),
    first(['cavok', 'visibility', 'visibilityMiles']),
    now.find((g) => g.kind === 'clouds' && g.feet !== null) ?? first(['verticalVisibility', 'sky']),
  ].filter((g): g is Group => !!g);
}
