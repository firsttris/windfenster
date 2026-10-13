---
description: How Windfenster cuts its own vector basemap from OpenStreetMap (Protomaps) and its relief from Mapterhorn, which zoom levels cost how much disk, and how to change them.
---

# The basemap and relief

Land, water, roads, towns and names under the wind come from **OpenStreetMap**, as vector tiles
from [Protomaps](https://protomaps.com/), drawn in Windfenster's own style (light and dark, names
in the viewer's language). The hill shading comes from [Mapterhorn](https://mapterhorn.com/). The
server cuts both itself and serves every tile; there is no map service and no API key.

## Three layers of detail

```mermaid
flowchart TB
  W["World to WF_BASEMAP_WORLD_ZOOM (7)<br>coasts, countries, towns"]
  R["Region to WF_BASEMAP_ZOOM (8)<br>Europe by default: main roads"]
  S["Spots to zoom 14, 20 km around<br>stations, opened places, tapped spots"]
  W --> R --> S
```

A **higher zoom is more detailed**: zoom 0 is the whole world on one tile, 7–8 shows countries and
main roads, 10–12 streets, forests and small lakes, 14 every path, building and slipway. Every
level splits each tile into four, so each costs three to four times the one before.

| Setting | Default | Covers | Size |
|---|---|---|---|
| `WF_BASEMAP_WORLD_ZOOM` | 7 | the whole world | 6: 45 MB · 7: about 150 MB |
| `WF_BASEMAP_REGION` | `-25,27,45,72` (Europe) | the deeper region, `west,south,east,north` | – |
| `WF_BASEMAP_ZOOM` | 8 | the region | Europe 8: about 190 MB · 12: about 7 GB · the Upper Rhine to 15: about 210 MB |
| `WF_BASEMAP_SPOTS` | 60 | detail areas kept | 2 to 17 MB each |
| `WF_TERRAIN_ZOOM` | 7 | relief, the whole world | 6: 278 MB · 7: 948 MB |

## How it is made

Towns, roads and lakes from close up come from [Protomaps](https://protomaps.com/), vector
tiles of OpenStreetMap, drawn in Windfenster's own style, light and dark, with names in the
viewer's language. Without them the map shows the world from
[Natural Earth](https://www.naturalearthdata.com/) (Europe finer).

By default (`WF_BASEMAP=auto`) the server makes the basemap itself, a minute after the start and
then checked daily: from Protomaps' newest daily build it takes, by byte range, the whole world
up to zoom 7 (coasts, countries, towns) and a region deeper, Europe up to zoom 8 by default
(main roads), into one file (together 323 MB, 24,752 tiles, about 13 minutes on 11 October),
`{WF_DATA_DIR}/map/basemap.pmtiles`. A note next to it (`basemap.json`) says what it was made
from; it is made again after 120 days, or when the settings change.

Detail (streets, lakes, beaches, up to zoom 14) comes where people look, anywhere in the world:
around each tracked station (`WF_TRACKED_STATIONS`), each place opened in the last week and each
spot tapped on the map, the server cuts 20 km from the newest build into a file of its own,
`{WF_DATA_DIR}/map/basemap-spots/{lat}_{lon}.pmtiles` (2 to 17 MB; spots are snapped to a 0.2°
grid, so places close together share one). It checks every minute and makes two at a time, so a
tapped spot has its detail within a minute or two, and the open map shows it without reloading.
At most 60 spots are kept (about 600 MB); beyond that the longest unwanted go.

The map gets every tile from the server, at `/api/map/basemap/{z}/{x}/{y}`: from the spot that
holds it, else from the main file, and where neither goes that deep, cut out of the deepest tile
above it (scaled and clipped, as MapLibre does past a source's last zoom). So the map is one
source with no seams and never blank; it is only coarser where nobody looked yet. With room to
spare, set `WF_BASEMAP_WORLD_ZOOM` and `WF_BASEMAP_ZOOM` higher (the region with
`WF_BASEMAP_REGION`), and `WF_TERRAIN_ZOOM` for finer relief.

Relief comes from [Mapterhorn](https://mapterhorn.com/) (elevation from national surveys and
other open data, [sources](https://mapterhorn.com/attribution)): the server cuts the whole world
to zoom 7 from its planet archive into `{WF_DATA_DIR}/map/terrain.pmtiles` (948 MB, 13,522 tiles, about 7 minutes on 11 October; renewed
yearly, served at `/api/map/terrain.pmtiles`), and the map draws it as hillshade over the colour
field and under water, roads and labels, softer from town zoom on.

A file you put there yourself (no note next to it) is never replaced. To cover more, cut one
with the [pmtiles CLI](https://docs.protomaps.com/pmtiles/cli) (or its Docker image
`protomaps/go-pmtiles`):

```bash
pmtiles extract https://build.protomaps.com/20261010.pmtiles data/map/basemap.pmtiles \
  --bbox=-25,27,45,72 --maxzoom=12
```

Europe to zoom 12 is about 7 GB; the Upper Rhine to zoom 15 about 210 MB. `WF_BASEMAP=off`
leaves the file to you; the server cuts its tiles the same way where it stops.
OpenStreetMap is credited on the map.

## Changing the depth

Change the variable and restart. A minute after the start the server compares the settings with
the note next to the file (`basemap.json`, `terrain.json`); when they differ it makes the file
anew from the newest build:

- it **downloads the whole file again** (no tiles are reused from the old one);
- it writes to `basemap.pmtiles.part` and swaps it in only when complete, so **the old map stays
  in use** meanwhile, and you need room for both;
- each range that fails for a moment (a 5xx, 429 or cut connection, which Protomaps and Mapterhorn
  answer now and then over hours of requests) is asked again up to five times, waiting 1 to 16 s;
  only when it keeps failing does the cut stop. A restart in the middle **starts over**; after an
  error it tries again in an hour;
- the detail spots stay as they are; from `WF_BASEMAP_ZOOM=14` none are made inside the region.

Reload the map page in the browser afterwards.

!!! tip "Full detail where you ride"
    A small region very deep costs little: `WF_BASEMAP_REGION=7.5,47.5,9.0,49.5` with
    `WF_BASEMAP_ZOOM=15` covers the Upper Rhine with every street and building in about 210 MB.
