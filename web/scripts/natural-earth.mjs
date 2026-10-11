// Builds the map's fallback basemap from Natural Earth (public domain): land, lakes, rivers,
// borders and places of Europe as one GeoJSON file, coordinates to 0.001°.
//
//   git clone --depth 1 --filter=blob:none --sparse https://github.com/nvkelso/natural-earth-vector ne
//   (cd ne && git sparse-checkout set --no-cone /geojson/ne_10m_land.geojson …)
//   node scripts/natural-earth.mjs ne/geojson src/assets/natural-earth.json
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const [dir, out] = process.argv.slice(2);
if (!dir || !out) throw new Error('usage: natural-earth.mjs <ne geojson dir> <out file>');

/** Europe with the Canaries, Iceland, Turkey and the Levant coast */
const BOX = { west: -25, south: 27, east: 45, north: 72 };
const read = (name) => JSON.parse(readFileSync(join(dir, `${name}.geojson`), 'utf8')).features;
const round = (x) => Math.round(x * 1000) / 1000;

/** Sutherland–Hodgman: a ring cut to the box, one edge after the other */
function clipRing(ring) {
  const edges = [(p) => p[0] >= BOX.west, (p) => p[0] <= BOX.east, (p) => p[1] >= BOX.south, (p) => p[1] <= BOX.north];
  const cut = [
    (a, b) => at(a, b, 0, BOX.west),
    (a, b) => at(a, b, 0, BOX.east),
    (a, b) => at(a, b, 1, BOX.south),
    (a, b) => at(a, b, 1, BOX.north),
  ];
  let pts = ring.slice(0, -1);
  edges.forEach((inside, e) => {
    const next = [];
    pts.forEach((b, i) => {
      const a = pts[(i + pts.length - 1) % pts.length];
      if (inside(b)) {
        if (!inside(a)) next.push(cut[e](a, b));
        next.push(b);
      } else if (inside(a)) next.push(cut[e](a, b));
    });
    pts = next;
  });
  if (pts.length < 3) return null;
  const rounded = dedupe(pts.map((p) => [round(p[0]), round(p[1])]));
  return rounded.length < 3 ? null : [...rounded, rounded[0]];
}

/** Where the segment a–b crosses coordinate `axis` = v */
function at(a, b, axis, v) {
  const t = (v - a[axis]) / (b[axis] - a[axis]);
  const p = [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])];
  p[axis] = v;
  return p;
}

function dedupe(pts) {
  return pts.filter((p, i) => i === 0 || p[0] !== pts[i - 1][0] || p[1] !== pts[i - 1][1]);
}

const polygons = (geom) => (geom.type === 'Polygon' ? [geom.coordinates] : geom.coordinates);
const lines = (geom) => (geom.type === 'LineString' ? [geom.coordinates] : geom.coordinates);
const inBox = ([x, y], m = 0) => x >= BOX.west - m && x <= BOX.east + m && y >= BOX.south - m && y <= BOX.north + m;

function clippedPolygons(features, props) {
  return features.flatMap((f) =>
    polygons(f.geometry)
      .map((rings) => rings.map(clipRing).filter(Boolean))
      .filter((rings) => rings.length > 0)
      .map((rings) => ({
        type: 'Feature',
        properties: props(f.properties),
        geometry: { type: 'Polygon', coordinates: rings },
      })),
  );
}

/** Lines with their points outside the box (plus a margin) dropped, split where they leave it */
function keptLines(features, props) {
  return features.flatMap((f) =>
    lines(f.geometry)
      .flatMap((line) => {
        const parts = [[]];
        for (const p of line) {
          if (inBox(p, 0.5)) parts[parts.length - 1].push([round(p[0]), round(p[1])]);
          else if (parts[parts.length - 1].length) parts.push([]);
        }
        return parts.map(dedupe).filter((l) => l.length > 1);
      })
      .map((l) => ({
        type: 'Feature',
        properties: props(f.properties),
        geometry: { type: 'LineString', coordinates: l },
      })),
  );
}

const features = [
  ...clippedPolygons(read('ne_10m_land'), () => ({ k: 'land' })),
  // The smallest lakes and brooks (ranks 11 and 12, rivers from 10) would double the file.
  ...clippedPolygons(
    [...read('ne_10m_lakes'), ...read('ne_10m_lakes_europe')].filter((f) => (f.properties.scalerank ?? 99) <= 10),
    (p) => ({ k: 'lake', r: p.scalerank ?? 10 }),
  ),
  ...keptLines(
    [...read('ne_10m_rivers_lake_centerlines'), ...read('ne_10m_rivers_europe')].filter(
      (f) => (f.properties.scalerank ?? 99) <= 9,
    ),
    (p) => ({ k: 'river', r: p.scalerank ?? 10 }),
  ),
  ...keptLines(read('ne_10m_admin_0_boundary_lines_land'), () => ({ k: 'border' })),
  ...read('ne_10m_populated_places')
    .filter((f) => inBox(f.geometry.coordinates) && f.properties.SCALERANK <= 8)
    .map((f) => ({
      type: 'Feature',
      properties: {
        k: 'place',
        r: f.properties.SCALERANK,
        cap: f.properties.ADM0CAP === 1 ? 1 : 0,
        de: f.properties.NAME_DE || f.properties.NAME,
        en: f.properties.NAME_EN || f.properties.NAME,
      },
      geometry: { type: 'Point', coordinates: f.geometry.coordinates.map(round) },
    })),
];
writeFileSync(out, JSON.stringify({ type: 'FeatureCollection', features }));
const kinds = {};
for (const f of features) kinds[f.properties.k] = (kinds[f.properties.k] ?? 0) + 1;
console.log(out, kinds);
