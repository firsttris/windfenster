/**
 * A METAR split into its groups (WMO FM 15 as used in Europe, plus the US forms that turn up).
 * The groups carry codes and numbers only; metar-text.ts puts them into words.
 */
import { MS_PER_KN } from './units';

export type Intensity = 'light' | 'heavy' | 'vicinity';
export type Cover = 'FEW' | 'SCT' | 'BKN' | 'OVC';
export type Trend = 'NOSIG' | 'BECMG' | 'TEMPO';

export type Group = { token: string } & (
  | { kind: 'time'; day: number; hour: number; minute: number }
  | { kind: 'auto' }
  | { kind: 'cor' }
  | { kind: 'nil' }
  /** `dir` null: variable (VRB) */
  | { kind: 'wind'; dir: number | null; kn: number; gustKn: number | null }
  | { kind: 'windVariation'; from: number; to: number }
  | { kind: 'cavok' }
  /** 9999 means 10 km or more (`atLeast`), 0000 less than 50 m */
  | { kind: 'visibility'; meters: number; atLeast: boolean; below: boolean }
  | { kind: 'visibilityMiles'; miles: number; atLeast: boolean }
  | { kind: 'rvr'; runway: string }
  | { kind: 'weather'; intensity: Intensity | null; recent: boolean; descriptor: string | null; phenomena: string[] }
  | { kind: 'notObserved' }
  /** `cover` null: not seen by the sensor (///) */
  | { kind: 'clouds'; cover: Cover | null; feet: number | null; type: 'CB' | 'TCU' | null }
  | { kind: 'verticalVisibility'; feet: number | null }
  | { kind: 'sky'; code: 'NSC' | 'NCD' | 'SKC' | 'CLR' | 'NSW' }
  | { kind: 'temperature'; temp: number; dew: number | null }
  | { kind: 'pressure'; hpa: number; inHg: number | null }
  | { kind: 'windShear' }
  | { kind: 'runwayState' }
  /** Water temperature, state of the sea (0–9) or wave height in metres */
  | { kind: 'sea'; temp: number | null; state: number | null; waveM: number | null }
  | { kind: 'colour'; code: string }
  | { kind: 'trend'; code: Trend }
  | { kind: 'trendTime'; prefix: 'FM' | 'TL' | 'AT'; hour: number; minute: number }
  | { kind: 'remarks' }
  | { kind: 'unknown' }
);

const DESCRIPTORS = ['MI', 'BC', 'PR', 'DR', 'BL', 'SH', 'TS', 'FZ'] as const;
const PHENOMENA = [
  'DZ',
  'RA',
  'SN',
  'SG',
  'IC',
  'PL',
  'GR',
  'GS',
  'UP',
  'BR',
  'FG',
  'FU',
  'VA',
  'DU',
  'SA',
  'HZ',
  'PO',
  'SQ',
  'FC',
  'SS',
  'DS',
] as const;
export type Descriptor = (typeof DESCRIPTORS)[number];
export type Phenomenon = (typeof PHENOMENA)[number];

const WEATHER = new RegExp(`^(\\+|-|VC|RE)?(${DESCRIPTORS.join('|')})?((?:${PHENOMENA.join('|')})*)$`);

/** "M05" → -5; "M00" (just below zero) → 0 */
const signed = (s: string) => (s.startsWith('M') ? -Number(s.slice(1)) : Number(s)) || 0;

type Rule = [RegExp, (x: RegExpMatchArray, token: string) => Group | null];

const SIMPLE: Record<string, Group['kind']> = { AUTO: 'auto', COR: 'cor', NIL: 'nil', CAVOK: 'cavok' };

/** Patterns in the order they are tried; the first that matches and returns a group wins. */
const RULES: Rule[] = [
  [/^(\d{2})(\d{2})(\d{2})Z$/, (x, token) => ({ token, kind: 'time', day: +x[1], hour: +x[2], minute: +x[3] })],
  [/^(NOSIG|BECMG|TEMPO)$/, (x, token) => ({ token, kind: 'trend', code: x[1] as Trend })],
  [
    /^(\d{3}|VRB)(\d{2,3})(?:G(\d{2,3}))?(KT|MPS|KMH)$/,
    (x, token) => {
      const toKn = { KT: 1, MPS: 1 / MS_PER_KN, KMH: 1 / 1.852 }[x[4] as 'KT' | 'MPS' | 'KMH'];
      const kn = Math.round(+x[2] * toKn);
      const dir = x[1] === 'VRB' || kn === 0 ? null : +x[1];
      return { token, kind: 'wind', dir, kn, gustKn: x[3] ? Math.round(+x[3] * toKn) : null };
    },
  ],
  [/^(\d{3})V(\d{3})$/, (x, token) => ({ token, kind: 'windVariation', from: +x[1], to: +x[2] })],
  [
    /^(\d{4})(?:NDV|[NSEW]{1,2})?$/,
    (x, token) => ({ token, kind: 'visibility', meters: +x[1], atLeast: x[1] === '9999', below: x[1] === '0000' }),
  ],
  [
    /^(P)?(\d+)(?:\/(\d+))?SM$/,
    (x, token) => ({ token, kind: 'visibilityMiles', miles: x[3] ? +x[2] / +x[3] : +x[2], atLeast: x[1] === 'P' }),
  ],
  [
    /^R(\d{2}[LCR]?)\/[PM]?\d{4}(?:V[PM]?\d{4})?(?:FT)?\/?[UDN]?$/,
    (x, token) => ({ token, kind: 'rvr', runway: x[1] }),
  ],
  [/^R\d{2}[LCR]?\/|^R\/SNOCLO$/, (_, token) => ({ token, kind: 'runwayState' })],
  [
    /^(FEW|SCT|BKN|OVC|\/\/\/)(\d{3}|\/\/\/)(CB|TCU|\/\/\/)?$/,
    (x, token) => ({
      token,
      kind: 'clouds',
      cover: x[1] === '///' ? null : (x[1] as Cover),
      feet: x[2] === '///' ? null : +x[2] * 100,
      type: x[3] === 'CB' || x[3] === 'TCU' ? x[3] : null,
    }),
  ],
  [
    /^VV(\d{3}|\/\/\/)$/,
    (x, token) => ({ token, kind: 'verticalVisibility', feet: x[1] === '///' ? null : +x[1] * 100 }),
  ],
  [/^(NSC|NCD|SKC|CLR|NSW)$/, (x, token) => ({ token, kind: 'sky', code: x[1] as 'NSC' })],
  [
    /^(M?\d{2})\/(M?\d{2})?$/,
    (x, token) => ({ token, kind: 'temperature', temp: signed(x[1]), dew: x[2] ? signed(x[2]) : null }),
  ],
  [/^Q(\d{4})$/, (x, token) => ({ token, kind: 'pressure', hpa: +x[1], inHg: null })],
  [
    /^A(\d{4})$/,
    (x, token) => ({ token, kind: 'pressure', hpa: Math.round((+x[1] / 100) * 33.8639), inHg: +x[1] / 100 }),
  ],
  [
    /^W(M?\d{2}|\/\/)\/(?:S(\d|\/)|H(\d+|\/+))$/,
    (x, token) => ({
      token,
      kind: 'sea',
      temp: x[1] === '//' ? null : signed(x[1]),
      state: x[2] === undefined || x[2] === '/' ? null : +x[2],
      waveM: x[3] === undefined || x[3].startsWith('/') ? null : +x[3] / 10,
    }),
  ],
  [/^(?:BLACK)?(?:BLU|WHT|GRN|YLO[12]?|AMB|RED)\+?$/, (_, token) => ({ token, kind: 'colour', code: token })],
  [
    /^(FM|TL|AT)(\d{2})(\d{2})$/,
    (x, token) => ({ token, kind: 'trendTime', prefix: x[1] as 'FM', hour: +x[2], minute: +x[3] }),
  ],
  // Sensor values missing: weather (//), recent weather, clouds, temperature, pressure
  [/^(?:RE)?\/+$|^Q\/{4}$/, (_, token) => ({ token, kind: 'notObserved' })],
  [
    WEATHER,
    (x, token) =>
      x[2] || x[3]
        ? {
            token,
            kind: 'weather',
            intensity:
              ({ '-': 'light', '+': 'heavy', VC: 'vicinity' } as Record<string, Intensity>)[x[1] ?? ''] ?? null,
            recent: x[1] === 'RE',
            descriptor: x[2] ?? null,
            phenomena: x[3].match(/../g) ?? [],
          }
        : null,
  ],
];

function group(token: string): Group {
  const simple = SIMPLE[token];
  if (simple) return { token, kind: simple } as Group;
  for (const [re, make] of RULES) {
    const x = token.match(re);
    const g = x && make(x, token);
    if (g) return g;
  }
  return { token, kind: 'unknown' };
}

/** The groups after the report type and station, in order; remarks (RMK …) as one group. */
export function decodeMetar(raw: string): Group[] {
  const tokens = raw.trim().replace(/=$/, '').split(/\s+/).filter(Boolean);
  let i = 0;
  // METAR / SPECI, an optional COR, the station
  while (i < tokens.length && /^(METAR|SPECI)$/.test(tokens[i])) i++;
  if (tokens[i] === 'COR') i++;
  if (/^[A-Z][A-Z0-9]{3}$/.test(tokens[i] ?? '')) i++;
  const out: Group[] = [];
  while (i < tokens.length) {
    const token = tokens[i++];
    if (token === 'RMK') {
      out.push({ token: tokens.slice(i - 1).join(' '), kind: 'remarks' });
      break;
    }
    if (token === 'WS') {
      // WS R24, WS ALL RWY
      const parts = [token];
      while (i < tokens.length && /^(R\d{2}[LCR]?|ALL|RWY)$/.test(tokens[i])) parts.push(tokens[i++]);
      out.push({ token: parts.join(' '), kind: 'windShear' });
      continue;
    }
    out.push(group(token));
  }
  return out;
}
