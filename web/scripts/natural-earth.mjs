// Builds the map's fallback basemap from Natural Earth (public domain): land, lakes, rivers,
// coasts, borders and places of the world as one GeoJSON file. Europe comes from the 1:10m data
// to 0.001°, the rest of the world from 1:50m to 0.01° with only its bigger places.
//
//   git clone --depth 1 --filter=blob:none --sparse https://github.com/nvkelso/natural-earth-vector ne
//   (cd ne && git sparse-checkout set --no-cone /geojson/ne_10m_land.geojson …)
//   node scripts/natural-earth.mjs ne/geojson src/assets/natural-earth.json
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const [dir, out] = process.argv.slice(2);
if (!dir || !out) throw new Error('usage: natural-earth.mjs <ne geojson dir> <out file>');

/** Europe with the Canaries, Iceland, Turkey and the Levant coast: the fine data */
const BOX = { west: -25, south: 27, east: 45, north: 72 };
/** The rest of the world as four boxes around Europe's, reaching this far into it so the coarse
 * and the fine land overlap instead of leaving a seam */
const OVERLAP = 0.25;
const OUTSIDE = [
  { west: -180, south: -90, east: BOX.west + OVERLAP, north: 90 },
  { west: BOX.east - OVERLAP, south: -90, east: 180, north: 90 },
  { west: BOX.west, south: -90, east: BOX.east, north: BOX.south + OVERLAP },
  { west: BOX.west, south: BOX.north - OVERLAP, east: BOX.east, north: 90 },
];
const read = (name) => JSON.parse(readFileSync(join(dir, `${name}.geojson`), 'utf8')).features;
const fine = (x) => Math.round(x * 1000) / 1000;
const coarse = (x) => Math.round(x * 100) / 100;

/** Sutherland–Hodgman: a ring cut to a box, one edge after the other */
function clipRing(ring, box, round) {
  const edges = [(p) => p[0] >= box.west, (p) => p[0] <= box.east, (p) => p[1] >= box.south, (p) => p[1] <= box.north];
  const cut = [
    (a, b) => at(a, b, 0, box.west),
    (a, b) => at(a, b, 0, box.east),
    (a, b) => at(a, b, 1, box.south),
    (a, b) => at(a, b, 1, box.north),
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
const inBox = ([x, y], box = BOX, m = 0) =>
  x >= box.west - m && x <= box.east + m && y >= box.south - m && y <= box.north + m;
const inEurope = (p) => inBox(p, BOX, 0.5);
const outsideEurope = (p) => !inBox(p, BOX);

function clippedPolygons(features, boxes, round, props) {
  return features.flatMap((f) =>
    boxes.flatMap((box) =>
      polygons(f.geometry)
        .map((rings) => rings.map((r) => clipRing(r, box, round)).filter(Boolean))
        .filter((rings) => rings.length > 0)
        .map((rings) => ({
          type: 'Feature',
          properties: props(f.properties),
          geometry: { type: 'Polygon', coordinates: rings },
        })),
    ),
  );
}

/** Lines with the points `keep` refuses dropped, split where they leave */
function keptLines(features, keep, round, props) {
  return features.flatMap((f) =>
    lines(f.geometry)
      .flatMap((line) => {
        const parts = [[]];
        for (const p of line) {
          if (keep(p)) parts[parts.length - 1].push([round(p[0]), round(p[1])]);
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

const rank = (p) => p.scalerank ?? 10;
/** A lake wholly outside Europe's box (the fine data has the others) */
const lakeOutside = (f) => polygons(f.geometry).every((rings) => rings[0].every(outsideEurope));

const places = (keep, maxRank) =>
  read('ne_10m_populated_places')
    .filter((f) => keep(f.geometry.coordinates) && f.properties.SCALERANK <= maxRank)
    .map((f) => ({
      type: 'Feature',
      properties: {
        k: 'place',
        r: f.properties.SCALERANK,
        cap: f.properties.ADM0CAP === 1 ? 1 : 0,
        de: f.properties.NAME_DE || f.properties.NAME,
        en: f.properties.NAME_EN || f.properties.NAME,
      },
      geometry: { type: 'Point', coordinates: f.geometry.coordinates.map(fine) },
    }));

const features = [
  // Land is only filled; its outline comes from the coastlines, so the boxes' edges draw none.
  ...clippedPolygons(read('ne_50m_land'), OUTSIDE, coarse, () => ({ k: 'land' })),
  ...clippedPolygons(read('ne_10m_land'), [BOX], fine, () => ({ k: 'land' })),
  ...read('ne_50m_lakes')
    .filter(lakeOutside)
    .flatMap((f) =>
      polygons(f.geometry).map((rings) => ({
        type: 'Feature',
        properties: { k: 'lake', r: rank(f.properties) },
        geometry: {
          type: 'Polygon',
          coordinates: rings.map((r) => dedupe(r.map((p) => [coarse(p[0]), coarse(p[1])]))),
        },
      })),
    ),
  // The smallest lakes and brooks (ranks 11 and 12, rivers from 10) would double the file.
  ...clippedPolygons(
    [...read('ne_10m_lakes'), ...read('ne_10m_lakes_europe')].filter((f) => rank(f.properties) <= 10),
    [BOX],
    fine,
    (p) => ({ k: 'lake', r: rank(p) }),
  ),
  ...keptLines(read('ne_50m_coastline'), outsideEurope, coarse, () => ({ k: 'coast' })),
  ...keptLines(
    read('ne_10m_coastline'),
    (p) => inBox(p),
    fine,
    () => ({ k: 'coast' }),
  ),
  ...keptLines(read('ne_50m_rivers_lake_centerlines'), outsideEurope, coarse, (p) => ({ k: 'river', r: rank(p) })),
  ...keptLines(
    [...read('ne_10m_rivers_lake_centerlines'), ...read('ne_10m_rivers_europe')].filter((f) => rank(f.properties) <= 9),
    inEurope,
    fine,
    (p) => ({ k: 'river', r: rank(p) }),
  ),
  ...keptLines(read('ne_50m_admin_0_boundary_lines_land'), outsideEurope, coarse, () => ({ k: 'border' })),
  ...keptLines(read('ne_10m_admin_0_boundary_lines_land'), inEurope, fine, () => ({ k: 'border' })),
  ...places((p) => inBox(p), 8),
  ...places(outsideEurope, 4),
];
writeFileSync(out, JSON.stringify({ type: 'FeatureCollection', features }));
const kinds = {};
for (const f of features) kinds[f.properties.k] = (kinds[f.properties.k] ?? 0) + 1;
console.log(out, kinds);
