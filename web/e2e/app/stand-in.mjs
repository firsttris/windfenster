/**
 * One stand-in for every outside source the server asks while the app tests run, in each
 * source's own format, under the paths `WF_SOURCES` puts them at (crates/windfenster/src/main.rs):
 * - /open-meteo/v1/forecast: Open-Meteo's forecast, every model asked for, ten days hourly
 * - /geocoding/v1/search: Open-Meteo's place search ("Rhein…" finds Rheinmünster)
 * - /awc/api/data/metar, /awc/api/data/taf, /awc/data/cache/stations.cache.json.gz:
 *   aviationweather.gov's METARs (every half hour), TAFs and station list
 * - /ourairports/airports.csv, /ourairports/regions.csv: OurAirports
 * - /dwd/stations.txt and the DWD's 10-minute wind and gust files (zip)
 * - /iem/cgi-bin/request/asos.py: the Iowa Environmental Mesonet's METAR archive, a year of
 *   hourly reports per request
 * - /dwd-hourly/historical/ and its long file: the DWD's archive of hourly wind
 * - /pegelonline/stations.json: PEGELONLINE's gauges with their current levels
 * Baden Airpark (EDSB) and Rhodes (LGRP) with METAR and TAF, Rheinstetten (DWD 04177). The
 * values follow the same weather as the map's textures (map-data.mjs), relative to `now`.
 */
import { createServer } from 'node:http';
import { gzipSync } from 'node:zlib';
import { wind } from './map-data.mjs';

const AIRPORTS = [
  {
    icao: 'EDSB',
    name: 'Karlsruhe Baden-Baden Airport',
    lat: 48.7794,
    lon: 8.0805,
    ft: 408,
    country: 'DE',
    region: 'DE-BW',
    town: 'Rheinmünster',
    keywords: 'Baden Airpark',
  },
  {
    icao: 'LGRP',
    name: 'Diagoras Airport',
    lat: 36.4054,
    lon: 28.0862,
    ft: 17,
    country: 'GR',
    region: 'GR-L',
    town: 'Rodos Island',
    keywords: 'Rhodes',
  },
];
const REGIONS = [
  ['DE-BW', 'Baden-Württemberg', 'DE'],
  ['GR-L', 'South Aegean', 'GR'],
];

const speedOf = (w) => Math.hypot(w.u, w.v);
const dirOf = (w) => Math.round(((Math.atan2(-w.u, -w.v) * 180) / Math.PI + 360) % 360);
const iso = (t) => new Date(t * 1000).toISOString();
const ddhhmm = (t) => iso(t).slice(8, 10) + iso(t).slice(11, 13) + iso(t).slice(14, 16);

/** Open-Meteo's forecast for the models asked: each a little off the others, ICON-D2 48 hours */
function forecast(q, now) {
  const [lat, lon] = [Number(q.get('latitude')), Number(q.get('longitude'))];
  const models = (q.get('models') ?? '').split(',').filter(Boolean);
  const start = Math.floor(now / 3600) * 3600 - 6 * 3600;
  const time = Array.from({ length: 240 }, (_, i) => start + i * 3600);
  const hourly = { time };
  models.forEach((m, k) => {
    const name = (v) => (models.length === 1 ? v : `${v}_${m}`);
    const ends = m === 'icon_d2' ? 48 : m.includes('arome') ? 42 : 240;
    const at = time.map((t, i) => (i < ends + 6 ? wind(lon, lat, (t - now) / 3600) : null));
    const off = 1 + (k - 2) * 0.06;
    hourly[name('wind_speed_10m')] = at.map((w) => (w ? Math.round(speedOf(w) * off * 10) / 10 : null));
    hourly[name('wind_gusts_10m')] = at.map((w) => (w ? Math.round((speedOf(w) * off * 1.4 + 4) * 10) / 10 : null));
    hourly[name('wind_direction_10m')] = at.map((w) => (w ? dirOf(w) : null));
    hourly[name('temperature_2m')] = at.map((w, i) =>
      w ? Math.round((14 + 4 * Math.sin((i / 24) * 2 * Math.PI)) * 10) / 10 : null,
    );
    hourly[name('precipitation')] = at.map((w, i) => (w ? (i % 17 < 3 ? 0.4 : 0) : null));
    hourly[name('cloud_cover')] = at.map((w, i) => (w ? (i * 7) % 100 : null));
  });
  return { timezone: 'Europe/Berlin', elevation: 120, hourly };
}

function metars(code, hours, now) {
  const a = AIRPORTS.find((x) => x.icao === code);
  if (!a) return [];
  const last = Math.floor(now / 1800) * 1800 - 600;
  return Array.from({ length: hours * 2 }, (_, i) => {
    const t = last - i * 1800;
    const w = wind(a.lon, a.lat, (t - now) / 3600);
    const [kn, dir] = [Math.round(speedOf(w)), Math.round(dirOf(w) / 10) * 10];
    const gust = kn >= 12 ? kn + 9 : null;
    const wnd = `${String(dir).padStart(3, '0')}${String(kn).padStart(2, '0')}${gust ? `G${gust}` : ''}KT`;
    return {
      icaoId: code,
      obsTime: t,
      temp: 14,
      wdir: dir,
      wspd: kn,
      wgst: gust,
      rawOb: `METAR ${code} ${ddhhmm(t)}Z ${wnd} CAVOK 14/09 Q1013`,
    };
  });
}

function tafs(code, now) {
  if (!AIRPORTS.some((a) => a.icao === code)) return [];
  const from = Math.floor(now / 3600) * 3600 - 2 * 3600;
  return [
    {
      icaoId: code,
      issueTime: iso(from - 1800),
      validTimeFrom: from,
      validTimeTo: from + 86400,
      rawTAF: `TAF ${code} ${ddhhmm(from - 1800)}Z 22012KT CAVOK TEMPO 22018G28KT BECMG 24010KT`,
      fcsts: [
        {
          timeFrom: from,
          timeTo: from + 43200,
          timeBec: null,
          fcstChange: null,
          probability: null,
          wdir: 220,
          wspd: 12,
          wgst: null,
          visib: '6+',
          wxString: null,
          clouds: [],
        },
        {
          timeFrom: from + 3600,
          timeTo: from + 14400,
          timeBec: null,
          fcstChange: 'TEMPO',
          probability: null,
          wdir: 220,
          wspd: 18,
          wgst: 28,
          visib: '6+',
          wxString: 'SHRA',
          clouds: [{ cover: 'BKN', base: 3000, type: 'CB' }],
        },
        {
          timeFrom: from + 43200,
          timeTo: from + 86400,
          timeBec: from + 50400,
          fcstChange: 'BECMG',
          probability: null,
          wdir: 240,
          wspd: 10,
          wgst: null,
          visib: '6+',
          wxString: null,
          clouds: [],
        },
      ],
    },
  ];
}

const csv = (rows) => rows.map((r) => r.map((v) => (typeof v === 'number' ? v : `"${v}"`)).join(',')).join('\n');

function airportsCsv() {
  const head = [
    'id',
    'ident',
    'type',
    'name',
    'latitude_deg',
    'longitude_deg',
    'elevation_ft',
    'continent',
    'iso_country',
    'iso_region',
    'municipality',
    'scheduled_service',
    'icao_code',
    'iata_code',
    'gps_code',
    'local_code',
    'home_link',
    'wikipedia_link',
    'keywords',
  ];
  const rows = AIRPORTS.map((a, i) => [
    i + 1,
    a.icao,
    'medium_airport',
    a.name,
    a.lat,
    a.lon,
    a.ft,
    'EU',
    a.country,
    a.region,
    a.town,
    'yes',
    a.icao,
    '',
    a.icao,
    '',
    '',
    '',
    a.keywords,
  ]);
  return `${head.map((h) => `"${h}"`).join(',')}\n${csv(rows)}\n`;
}

function regionsCsv() {
  const rows = REGIONS.map(([code, name, country], i) => [
    i + 1,
    code,
    code.split('-')[1],
    name,
    'EU',
    country,
    '',
    '',
  ]);
  return `"id","code","local_code","name","continent","iso_country","wikipedia_link","keywords"\n${csv(rows)}\n`;
}

function dwdStations(now) {
  const today = iso(now).slice(0, 10).replaceAll('-', '');
  return [
    'Stations_id von_datum bis_datum Stationshoehe geoBreite geoLaenge Stationsname Bundesland Abgabe',
    '----------- --------- --------- ------------- --------- --------- ----------------------------------------- ---------- ------',
    `04177 20081028 ${today}            116     48.9726    8.3301 Rheinstetten                             Baden-Württemberg                        Frei`,
  ].join('\n');
}

/** The DWD's 10-minute file: `;`-separated, MESS_DATUM in UTC, m/s */
function dwdFile(kind, now, days) {
  const end = Math.floor(now / 600) * 600 - 600;
  const lines = [
    kind === 'wind'
      ? 'STATIONS_ID;MESS_DATUM;  QN;FF_10;DD_10;eor'
      : 'STATIONS_ID;MESS_DATUM;  QN;FX_10;FNX_10;FMX_10;DX_10;eor',
  ];
  for (let t = end - days * 86400; t <= end; t += 600) {
    const w = wind(8.3301, 48.9726, (t - now) / 3600);
    const ms = (speedOf(w) * 0.514444).toFixed(1);
    const stamp = iso(t).slice(0, 16).replace(/[-T:]/g, '');
    lines.push(
      kind === 'wind'
        ? `4177;${stamp};    2;   ${ms}; ${dirOf(w)};eor`
        : `4177;${stamp};    2;   ${(ms * 1.5).toFixed(1)};   1.0;   ${ms}; ${dirOf(w)};eor`,
    );
  }
  return Buffer.from(`${lines.join('\n')}\n`, 'latin1');
}

const crcTable = Array.from({ length: 256 }, (_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});
function crc32(buf) {
  let c = 0xffffffff;
  for (const b of buf) c = crcTable[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

/** A zip of one file, stored uncompressed */
function zip(name, data) {
  const n = Buffer.from(name);
  const crc = crc32(data);
  const local = Buffer.alloc(30);
  local.writeUInt32LE(0x04034b50, 0);
  local.writeUInt16LE(20, 4);
  local.writeUInt32LE(crc, 14);
  local.writeUInt32LE(data.length, 18);
  local.writeUInt32LE(data.length, 22);
  local.writeUInt16LE(n.length, 26);
  const central = Buffer.alloc(46);
  central.writeUInt32LE(0x02014b50, 0);
  central.writeUInt16LE(20, 4);
  central.writeUInt16LE(20, 6);
  central.writeUInt32LE(crc, 16);
  central.writeUInt32LE(data.length, 20);
  central.writeUInt32LE(data.length, 24);
  central.writeUInt16LE(n.length, 28);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0);
  end.writeUInt16LE(1, 8);
  end.writeUInt16LE(1, 10);
  end.writeUInt32LE(46 + n.length, 12);
  end.writeUInt32LE(30 + n.length + data.length, 16);
  return Buffer.concat([local, n, data, central, n, end]);
}

/** A past hour's wind for the statistics: a westerly that is strong in summer at Rhodes and in
 * winter at Baden Airpark and Rheinstetten, by day more than by night */
function pastWind(lat, t) {
  const d = new Date(t * 1000);
  const month = d.getUTCMonth();
  const hour = d.getUTCHours();
  const summer = Math.cos(((month - 6.5) / 12) * 2 * Math.PI);
  const season = lat < 40 ? 0.5 + 0.5 * summer : 0.5 - 0.5 * summer;
  const kn = (6 + 14 * season) * (hour >= 8 && hour < 18 ? 1.2 : 0.6);
  return { kn: Math.round(kn * 10) / 10, dir: lat < 40 ? 250 : 230 };
}

function iemYear(code, year) {
  const a = AIRPORTS.find((x) => x.icao === code);
  const rows = ['station,valid,sknt,drct'];
  if (!a) return `${rows[0]}\n`;
  for (let t = Date.UTC(year, 0, 1) / 1000; t < Date.UTC(year + 1, 0, 1) / 1000; t += 3600) {
    const w = pastWind(a.lat, t);
    rows.push(`${code},${iso(t).slice(0, 10)} ${iso(t).slice(11, 13)}:50,${w.kn.toFixed(2)},${w.dir.toFixed(2)}`);
  }
  return `${rows.join('\n')}\n`;
}

/** The DWD's hourly archive of Rheinstetten: from 2008 to the end of last year */
function dwdHourly(now) {
  const last = new Date(now * 1000).getUTCFullYear() - 1;
  const lines = ['STATIONS_ID;MESS_DATUM;QN_3;   F;   D;eor'];
  for (let t = Date.UTC(last - 11, 0, 1) / 1000; t < Date.UTC(last + 1, 0, 1) / 1000; t += 3600) {
    const w = pastWind(48.97, t);
    const stamp = iso(t).slice(0, 13).replace(/[-T]/g, '');
    lines.push(`       4177;${stamp};   10;   ${(w.kn * 0.514444).toFixed(1)}; ${w.dir};eor`);
  }
  return { name: `stundenwerte_FF_04177_${last - 11}0101_${last}1231_hist.zip`, text: `${lines.join('\n')}\n` };
}

/** PEGELONLINE's gauge list with current values, as the server asks for it: Iffezheim on the
 * Rhine high, Maxau normal, a gauge abroad without coordinates (left out) */
function gauges(now) {
  const at = iso(now - 900).replace('Z', '+00:00');
  const series = (value, state) => [
    {
      shortname: 'W',
      unit: 'cm',
      currentMeasurement: { timestamp: at, value, stateMnwMhw: state, stateNswHsw: 'normal' },
    },
  ];
  return [
    {
      uuid: 'b02be240-1364-4c97-8bb6-675d7d842332',
      number: '23500600',
      longname: 'IFFEZHEIM',
      km: 336.2,
      latitude: 48.852149,
      longitude: 8.116902,
      water: { shortname: 'RHEIN', longname: 'RHEIN' },
      timeseries: series(612, 'high'),
    },
    {
      uuid: 'a0b0a6fd-1d4e-4f39-8b4a-3b5f0f5e0c1a',
      number: '23700200',
      longname: 'MAXAU',
      km: 362.3,
      latitude: 49.03,
      longitude: 8.3,
      water: { shortname: 'RHEIN', longname: 'RHEIN' },
      timeseries: series(498, 'normal'),
    },
    { uuid: 'x', number: '9', longname: 'ABROAD', water: null, timeseries: [] },
  ];
}

/** Starts the stand-in; `now` (unix seconds) is the moment the tests take as now. */
export function startStandIn(port, now) {
  const server = createServer((req, res) => {
    const url = new URL(req.url ?? '/', 'http://stand-in');
    const q = url.searchParams;
    const json = (body) => {
      res.writeHead(200, { 'content-type': 'application/json' });
      res.end(JSON.stringify(body));
    };
    const send = (body, type) => {
      res.writeHead(200, { 'content-type': type });
      res.end(body);
    };
    const p = url.pathname;
    let m = null;
    if (p === '/open-meteo/v1/forecast') return json(forecast(q, now));
    if (p === '/geocoding/v1/search') {
      const name = (q.get('name') ?? '').toLowerCase();
      if (name.startsWith('rhein'))
        return json({
          results: [
            { name: 'Rheinmünster', latitude: 48.77, longitude: 8.05, country_code: 'DE', admin1: 'Baden-Württemberg' },
          ],
        });
      return json({ generationtime_ms: 0.1 });
    }
    if (p === '/awc/api/data/metar') return json(metars(q.get('ids'), Number(q.get('hours') ?? 24), now));
    if (p === '/awc/api/data/taf') return json(tafs(q.get('ids'), now));
    if (p === '/awc/data/cache/stations.cache.json.gz') {
      const list = AIRPORTS.map((a) => ({
        id: a.icao,
        icaoId: a.icao,
        site: a.name,
        lat: a.lat,
        lon: a.lon,
        country: a.country,
        siteType: ['METAR', 'TAF'],
      }));
      return send(gzipSync(JSON.stringify(list)), 'application/gzip');
    }
    if (p === '/pegelonline/stations.json') return json(gauges(now));
    if (p === '/ourairports/airports.csv') return send(airportsCsv(), 'text/csv');
    if (p === '/ourairports/regions.csv') return send(regionsCsv(), 'text/csv');
    if (p === '/dwd/stations.txt') return send(Buffer.from(dwdStations(now), 'latin1'), 'text/plain');
    m = p.match(
      /^\/dwd\/(wind|extreme_wind)\/(now|recent)\/10minutenwerte_(?:wind|extrema_wind)_04177_(?:now|akt)\.zip$/,
    );
    if (m) {
      const kind = m[1] === 'wind' ? 'wind' : 'gust';
      return send(zip(`produkt_${kind}.txt`, dwdFile(kind, now, m[2] === 'now' ? 1 : 10)), 'application/zip');
    }
    if (p === '/iem/cgi-bin/request/asos.py')
      return send(iemYear(q.get('station'), Number(q.get('sts')?.slice(0, 4))), 'text/plain');
    if (p === '/dwd-hourly/historical/') {
      const { name } = dwdHourly(now);
      return send(`<a href="${name}">${name}</a>`, 'text/html');
    }
    if (p === `/dwd-hourly/historical/${dwdHourly(now).name}`) {
      const { text } = dwdHourly(now);
      return send(zip('produkt_ff_stunde_04177.txt', Buffer.from(text, 'latin1')), 'application/zip');
    }
    res.writeHead(404);
    res.end();
  });
  return new Promise((resolve) => server.listen(port, '127.0.0.1', () => resolve(server)));
}
