/**
 * Wind map data for the app tests, written straight into the server's data directory as the
 * server writes it (crates/wf-map/src/store.rs and texture.rs): per model and run a folder
 * `{model}/{YYYYMMDDHH}` with `{step}.json` (TextureMeta), `{step}.rgba` (the texels) and, but for
 * ICON global, which the app takes in pieces, `{step}.png`. Small grids instead of the models'
 * own, so nothing is downloaded and the tests start in seconds.
 */
import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { deflateSync } from 'node:zlib';

const RANGE = { uMin: -40, uMax: 40, vMin: -40, vMax: 40 };
const GUST_MAX = 60;

/** A low moving in from west of Ireland, stronger by day: the wind in kn at a place, `hours`
 * from now */
export function wind(lon, lat, hours) {
  const [cx, cy] = [-12 + hours * 0.3, 54];
  const [dx, dy] = [(lon - cx) * Math.cos((lat * Math.PI) / 180), lat - cy];
  const r = Math.hypot(dx, dy) || 1e-6;
  // Stronger in the afternoon, weaker at night
  const speed = 28 * (r / 6) * Math.exp(1 - r / 6) * (1 + 0.35 * Math.sin(((hours - 9) / 24) * 2 * Math.PI));
  return { u: 6 - (speed * dy) / r, v: (speed * dx) / r };
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
function chunk(type, data) {
  const out = Buffer.alloc(12 + data.length);
  out.writeUInt32BE(data.length, 0);
  out.write(type, 4, 'latin1');
  data.copy(out, 8);
  out.writeUInt32BE(crc32(out.subarray(4, 8 + data.length)), 8 + data.length);
  return out;
}
/** An RGBA PNG, rows unfiltered */
function png(rgba, width, height) {
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr.set([8, 6, 0, 0, 0], 8);
  const raw = Buffer.alloc((width * 4 + 1) * height);
  for (let y = 0; y < height; y++)
    Buffer.from(rgba.buffer, y * width * 4, width * 4).copy(raw, y * (width * 4 + 1) + 1);
  return Buffer.concat([
    Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
    chunk('IHDR', ihdr),
    chunk('IDAT', deflateSync(raw)),
    chunk('IEND', Buffer.alloc(0)),
  ]);
}

const pad = (n) => String(n).padStart(2, '0');
function runId(t) {
  const d = new Date(t * 1000);
  return `${d.getUTCFullYear()}${pad(d.getUTCMonth() + 1)}${pad(d.getUTCDate())}${pad(d.getUTCHours())}`;
}

/** One model run: `steps` hours after `runAt` on a grid of `step`° from (west, south) to
 * (east, north) at texel edges; `inside(lon, lat)` false outside the model's domain. */
function writeRun(root, model, runAt, steps, [west, south, east, north], step, o = {}) {
  const width = Math.round((east - west) / step);
  const height = Math.round((north - south) / step);
  const dir = join(root, model, runId(runAt));
  mkdirSync(dir, { recursive: true });
  const byte = (x, min, max) => Math.max(0, Math.min(255, Math.round(((x - min) / (max - min)) * 255)));
  for (const s of steps) {
    const rgba = new Uint8Array(width * height * 4);
    for (let j = 0; j < height; j++) {
      for (let i = 0; i < width; i++) {
        const [lon, lat] = [west + (i + 0.5) * step, north - (j + 0.5) * step];
        if (o.inside && !o.inside(lon, lat)) continue;
        const w = wind(lon, lat, (runAt - o.now) / 3600 + s);
        const gust = 1.4 * Math.hypot(w.u, w.v) + 4;
        const k = (j * width + i) * 4;
        rgba.set(
          [byte(w.u, RANGE.uMin, RANGE.uMax), byte(w.v, RANGE.vMin, RANGE.vMax), byte(gust, 0, GUST_MAX), 255],
          k,
        );
      }
    }
    const meta = {
      model,
      runAt,
      step: s,
      validAt: runAt + s * 3600,
      west,
      south,
      east,
      north,
      width,
      height,
      domain: o.domain ?? [],
      ...RANGE,
      gustMax: GUST_MAX,
    };
    const name = String(s).padStart(3, '0');
    writeFileSync(join(dir, `${name}.json`), JSON.stringify(meta));
    writeFileSync(join(dir, `${name}.rgba`), rgba);
    if (!o.pieces) writeFileSync(join(dir, `${name}.png`), png(rgba, width, height));
  }
}

/** ICON-D2 (its real domain, 0.1°, 48 hours), ICON global (1°, 24 hours) and ECMWF (1°, every
 * third hour to 120), from runs that just came out at `now` (unix seconds). */
export function writeMapData(root, now) {
  const hour = (h) => Math.floor(now / (h * 3600)) * h * 3600;
  const range = (from, to, by = 1) => Array.from({ length: Math.floor((to - from) / by) + 1 }, (_, i) => from + i * by);
  const d2 = [-3.94, 43.18, 20.34, 58.08];
  writeRun(root, 'icon_d2', hour(3) - 3 * 3600, range(0, 48), [-4, 43.1, 20.4, 58.1], 0.1, {
    now,
    inside: (lon, lat) => lon >= d2[0] && lon <= d2[2] && lat >= d2[1] && lat <= d2[3],
    domain: [
      [d2[0], d2[1]],
      [d2[2], d2[1]],
      [d2[2], d2[3]],
      [d2[0], d2[3]],
    ],
  });
  writeRun(root, 'icon_global', hour(12) - 12 * 3600, range(0, 36), [-180.5, -90.5, 179.5, 90.5], 1, {
    now,
    pieces: true,
  });
  writeRun(root, 'ecmwf_ifs025', hour(6) - 6 * 3600, range(0, 120, 3), [-180.5, -90.5, 179.5, 90.5], 1, { now });
}
