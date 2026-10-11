/** A TAF's change groups in words: when (station time) and what (wind, visibility, weather). */
import type { Taf } from '@/bindings/Taf';
import type { TafGroup } from '@/bindings/TafGroup';
import type { TafHour } from '@/bindings/TafHour';
import { m } from '@/paraglide/messages';
import { localClock } from './forecast';
import { compass, formatNumber } from './format';
import { intlLocale } from './i18n';
import { decodeGroups } from './metar';
import { type Context, describe } from './metar-text';
import { formatWind, UNIT_LABEL } from './units';

/** "14:00", "So 14:00"; "14–18", "So 08–11" when the day is not today (station time). */
function when(from: number, to: number | null, now: number, timeZone: string): string {
  const clock = localClock(timeZone);
  const today = clock(now).date;
  const day = (unix: number) =>
    clock(unix).date === today
      ? ''
      : `${new Intl.DateTimeFormat(intlLocale(), { weekday: 'short', timeZone }).format(new Date(unix * 1000))} `;
  const hh = (unix: number) => String(clock(unix).hour).padStart(2, '0');
  if (to === null) return `${day(from)}${hh(from)}:00`;
  const end = clock(to).date === clock(from).date ? hh(to) : `${day(to)}${hh(to)}`.trim();
  return `${day(from)}${hh(from)}–${end}`;
}

export function groupWhen(g: TafGroup, now: number, timeZone: string): string {
  switch (g.change) {
    case 'base':
      return m.taf_until({ time: when(g.to, null, now, timeZone) });
    case 'from':
      return m.taf_from({ time: when(g.from, null, now, timeZone) });
    case 'becoming':
      return m.taf_from({ time: when(g.from, g.by ?? g.to, now, timeZone) });
    case 'tempo': {
      const time = when(g.from, g.to, now, timeZone);
      return g.probability ? m.taf_probTempo({ probability: g.probability, time }) : m.taf_tempo({ time });
    }
    case 'prob':
      return m.taf_prob({ probability: g.probability ?? 0, time: when(g.from, g.to, now, timeZone) });
  }
}

const speed = (kn: number, ctx: Context) => `${formatWind(kn, ctx.unit)} ${UNIT_LABEL[ctx.unit]}`;

/** The prevailing group before each group: the base, then FM and BECMG in order. */
export function prevailingBefore(groups: readonly TafGroup[]): (TafGroup | null)[] {
  let prev: TafGroup | null = null;
  return groups.map((g) => {
    const before = prev;
    if (g.change === 'base' || g.change === 'from' || g.change === 'becoming') prev = g;
    return before;
  });
}

const sameClouds = (a: TafGroup, b: TafGroup) => JSON.stringify(a.clouds) === JSON.stringify(b.clouds);

/**
 * What a group forecasts. aviationweather.gov carries visibility, weather and clouds of the
 * prevailing conditions into each change group; only what differs from `prev` is told.
 */
export function groupWhat(g: TafGroup, ctx: Context, prev: TafGroup | null = null): string {
  const parts: string[] = [];
  if (g.wind !== null) {
    let wind: string;
    if (g.wind === 0) wind = m.metar_calm();
    else if (g.variable || g.dir === null) wind = m.taf_windVariable({ speed: speed(g.wind, ctx) });
    else wind = m.taf_wind({ compass: compass(g.dir), deg: Math.round(g.dir), speed: speed(g.wind, ctx) });
    parts.push(g.gust === null ? wind : m.metar_gusts({ wind, gust: speed(g.gust, ctx) }));
  }
  if (g.visibilityM !== null && g.visibilityM !== prev?.visibilityM) {
    parts.push(
      g.visibilityM >= 9999
        ? m.metar_visibilityMax()
        : m.metar_visibility({
            distance:
              g.visibilityM >= 5000 ? `${formatNumber(g.visibilityM / 1000)} km` : `${formatNumber(g.visibilityM)} m`,
          }),
    );
  }
  // NSW (no significant weather) only means something after weather.
  const hadWeather = !!prev?.weather && prev.weather !== 'NSW';
  if (g.weather && g.weather !== prev?.weather && (g.weather !== 'NSW' || hadWeather)) {
    for (const w of decodeGroups(g.weather)) parts.push(describe(w, ctx));
  }
  if (!prev || !sameClouds(g, prev)) {
    // Low ceilings and shower or thunder clouds; few and scattered clouds only when the group
    // says nothing else.
    const told = parts.length;
    const significant = g.clouds.filter((c) => c.kind || c.cover === 'BKN' || c.cover === 'OVC');
    for (const c of told || significant.length ? significant : g.clouds) {
      const base = c.baseFt === null ? '///' : String(Math.round(c.baseFt / 100)).padStart(3, '0');
      for (const cg of decodeGroups(`${c.cover}${base}${c.kind ?? ''}`)) parts.push(describe(cg, ctx));
    }
  }
  return parts.join(', ') || '–';
}

/** The TAF's value for an hour (Unix seconds on the hour). */
export function tafHour(taf: Taf, at: number): TafHour | null {
  return taf.hours.find((h) => h.at === at) ?? null;
}
