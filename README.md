<div align="center">

<img src="web/public/favicon.svg" alt="Windfenster logo" width="64">

<h1>Windfenster</h1>

**Open-source wind forecast for water sports.**<br>
Every station shows what it measures right now, what the models predict, and which model
was closest to the measurements there recently.

[![CI](https://github.com/firsttris/windfenster/actions/workflows/ci.yml/badge.svg)](https://github.com/firsttris/windfenster/actions/workflows/ci.yml)
[![License: AGPL-3.0](https://img.shields.io/badge/license-AGPL--3.0-blue)](LICENSE)

</div>

> **Status:** early development. Station and point pages with forecasts from five models work
> (Phase 1). Phase 2 adds live measurements from METAR and DWD stations, airport forecasts
> (TAF), water gauges, the measurements of the last ten days, the model ranking per station,
> the Windfenster mix and the comparison chart. Phase 3, under way: the wind map with ICON-D2
> as colour field and moving particles, a timeline and the stations' live wind.

## What v1 will do

- A page for every METAR and DWD station: live measurement, 10-day table (wind, gusts,
  direction, weather, temperature, rain) in kn, m/s or km/h
- A forecast for any point by map click or search, with the nearest station
- Superforecast / Forecast plus a free choice of at least five models
- A model ranking per station from 30 days of verification, and the automatic model choice
  that follows from it
- The Windfenster mix: a forecast per station, weighted by how well each model hit there
- Water levels and estimated current on rivers (PEGELONLINE)
- An animated wind map, favourites, the best window for wing or kite size, alarms via web push
- Self-hosting as a single Docker container; an Open-Meteo API key is optional

## Quick start

```bash
curl -O https://raw.githubusercontent.com/firsttris/windfenster/main/compose.yaml
docker compose up -d
```

Open http://localhost:8080. The database lives in `./data`. Until the first release is out,
build the image yourself: `docker compose up --build` in a checkout.

| Variable | Default | |
|---|---|---|
| `WF_DATA_DIR` | `/data` | Database and map textures |
| `WF_LISTEN` | `0.0.0.0:8080` | Address of the HTTP server |
| `WF_PUBLIC_URL` | | Public address, e.g. `https://windfenster.example`, for canonical links of station pages |
| `OPEN_METEO_API_KEY` | | Open-Meteo key; without one the free, non-commercial API is used |
| `OPEN_METEO_BASE_URL` | | Own Open-Meteo instance |
| `OPEN_METEO_GEOCODING_URL` | | Own Open-Meteo geocoding instance (place search) |
| `WF_OPEN_METEO_DAILY_BUDGET` | 9,000 without key | Open-Meteo calls per day |
| `WF_MODELS` | all | Models to use, comma-separated Open-Meteo ids |
| `WF_TRACKED_STATIONS` | `edsb,lgrp` | Stations (slugs) kept current and verified even when nobody opens them |
| `WF_SCHEDULER` | `true` | Background jobs: station lists weekly, model runs every 5 minutes, METARs and water gauges every 10, TAFs and DWD measurements every 30, verification hourly |
| `WF_MAP` | `true` | The wind map: ICON-D2 from the DWD and ECMWF's IFS, checked every 10 minutes, see below; `false` turns it off |
| `WF_BASEMAP` | `auto` | The map's detailed basemap: `auto` makes it from Protomaps' daily build for the region and the tracked stations, unless you put one in place; `off` leaves it to you |
| `RUST_LOG` | `info,sqlx=warn` | Log level |

Commands of the binary:

- `windfenster serve` (default): migrates the database, serves the API and the web app, keeps
  station lists and forecasts current
- `windfenster migrate`: migrates and exits
- `windfenster import-stations`: fetches the station lists now
- `windfenster probe --lat 48.78 --lon 8.08`: asks Open-Meteo for every model at a point and
  prints what comes back

### How forecasts are fetched

Requests go to Open-Meteo only from the server. A place (a station or a point rounded to
0.01°) is fetched when someone opens it, once per model run. Places opened within the last
seven days are refreshed after every new run; free points are dropped two days after the last
request. The free API allows about 10,000 calls a day, which covers a few hundred places
that people look at regularly; larger instances need a key or an own Open-Meteo.

### How measurements are fetched

Every ten minutes the server reads aviationweather.gov's cache of all current METARs (one
file of about 250 KB) and keeps the reports of its stations. DWD stations deliver 10-minute
values; they are fetched every 30 minutes for the tracked stations and those opened within
the last seven days, and right away when someone opens a station whose values are stale.
TAFs (airport forecasts) of the same stations come from aviationweather.gov's TAF API every 30
minutes, with the change groups already decoded; Windfenster turns them into an hourly
series with the span that TEMPO, PROB and a change in progress allow. Measurements and TAFs
are kept for 13 months, for the verification of the models.

Each station page shows its measurements over a day, as Windfinder does: the last 24 hours
and the ten days before, with mean wind, gusts and direction. When a station's past days
are thin, the server fills them in once: the METAR API keeps about eight days, the DWD's
archive of 10-minute values about a year and a half (kept here for 13 months).

Water gauges come from PEGELONLINE: one request every 10 minutes brings all of them (about 730
with coordinates) with level, flow, water temperature and the official flood states. A
station or point within 15 km of a gauge shows it, with the last seven days fetched from
PEGELONLINE when the page is opened. The current on the Rhine is estimated from the flow and
a blanket cross-section (300 m × 6 m, surface 1.3 times the mean) and labelled as an estimate.

### How models are verified

Every hour Windfenster compares, for each station with measurements and stored forecasts,
every forecast hour with the measurement closest to it (within 20 minutes): the mean wind
(absolute error and bias in kn), the direction (angle, from 5 kn measured on) and whether
both said "14 kn or more". A run counts only for the hours after Windfenster fetched it, so
a model that arrives late gains nothing from hours that had passed. Errors are kept per
station, model and UTC day, split by lead time (0–24, 24–48 and 48–72 hours after the run);
the TAF counts as a model of its own, from its issue time.

- **Ranking** ("Who hits here?"): the mean absolute error of the first day ahead over the
  last 30 days. A point takes the ranking of a station at most 5 km away.
- **Auto choice**: from 14 days of data the page opens with the regional model of the lowest
  error (0–24 h), and Forecast takes the global model of the lowest error (48–72 h). Before
  that, a fixed order decides.
- **Windfenster mix**: per hour the weighted mean of every model with a value, weights 1/MAE²
  of the model at the station for that lead time (all alike before 14 days); the direction is
  that of the mean wind vector. It is computed when a page is served and verified like a
  model, as it stood each time new runs came in. It opens the page only where it beat the best
  single model.
- **Do the models agree?** The spread (highest minus lowest mean wind) of the models at the
  windiest daytime hour of the next two days: up to 4 kn high, up to 8 kn medium, above that
  low. The trend days 8–10 show the same as a dot.

### How the wind map is fed

The map draws the model fields themselves, not Open-Meteo's point series. Every 10 minutes
Windfenster looks for the newest ICON-D2 run on [DWD Open Data](https://opendata.dwd.de/)
(2.2 km, Germany and the Alps, a run every 3 hours, 48 hours ahead) and fetches the wind at
10 m (U and V, GRIB2) and the gusts (`vmax_10m`) for the hours that are out and not on disk yet.
Each hour becomes a PNG texture in kn (red U, green V, as in
[mapbox/webgl-wind](https://github.com/mapbox/webgl-wind), blue the gusts) with a JSON file for
its extent, value ranges and the model's domain, under
`{WF_DATA_DIR}/map/icon_d2/{run}/{hour}`. Under it, for all of Europe and beyond ICON-D2's 48
hours, lies ECMWF's IFS from [ECMWF open data](https://www.ecmwf.int/en/forecasts/datasets/open-data)
(0.25°, a run every 6 hours): Windfenster reads each step's index and fetches only the wind at
10 m and the gusts (`10fg`) by byte range (2.8 MB per step), for 0 to 120 hours every 3 hours,
and cuts them to the map's region (25° W to 45° E, 27° N to 72° N). When ECMWF's portal is busy
(HTTP 429) it waits 20 seconds and asks again, a few times, then leaves the rest to the next
round. The newest run and the one before are kept per model: for ICON-D2 about 150 MB of
downloads and 55 MB on disk per run, for ECMWF about 115 MB and 4 MB. Hours without gusts
(ECMWF's first, and late hours of some runs) show the wind on the gust layer, with a note.
`GET /api/map` lists the hours, each from the newest run that has it.

The map page (`/map`) draws ICON-D2 over ECMWF, blended over the last 30 to 60 km to its
domain's edge, and ECMWF alone where and when ICON-D2 has nothing; the other model can be picked
instead. It draws the field, wind or gusts, as a colour layer under coastlines and labels and
the particles on a canvas of their own, after mapbox/webgl-wind: a shader moves them along the
wind of the hour shown, blended between two model hours while the timeline plays; fewer of them
when the device cannot keep up. From town zoom on (zoom 9.5 to 11.5) the field fades out and
the map comes forward, with forests, meadows, water, industry, airfields and buildings; the
particles then take the field's colours, so the wind's strength stays readable on the map.
Tap a spot for the model's wind and gusts there, tap a station for its last measurement too;
stations show their measured wind as a dot in the wind scale's colour, from closer up as an
arrow with the value; from afar they are bundled, each bundle showing the strongest wind
measured in it in that colour (tap it to zoom in). Lakes and rivers lie dark over the field. The line under the models names the model and run drawing at the spot
picked or the middle of the view. The view stays in the address (`/map#zoom/lat/lon`). With
"reduce motion" set in the system, arrows that stand still take the particles' place. The land, water, borders and places under it come from
the detailed basemap if one is installed, else from
[Natural Earth](https://www.naturalearthdata.com/) (`web/scripts/natural-earth.mjs` builds that).

### The detailed basemap

Towns, roads and lakes from close up come from [Protomaps](https://protomaps.com/), vector
tiles of OpenStreetMap in one file, `{WF_DATA_DIR}/map/basemap.pmtiles`, drawn in Windfenster's
own style, light and dark, with names in the viewer's language. Without it the map shows
Natural Earth.

By default (`WF_BASEMAP=auto`) the server makes the file itself, a minute after the start and
then checked daily: from Protomaps' newest daily build it takes the map's region up to zoom 8
(coasts, towns, main roads; about 190 MB) and 20 km around each tracked station
(`WF_TRACKED_STATIONS`) up to zoom 14 (streets, lakes, beaches), by byte range, into one file. A
note next to it (`basemap.json`) says what it was made from; it is made again when the tracked
stations change or after 120 days.

A file you put there yourself (no note next to it) is never replaced. To cover more, cut one
with the [pmtiles CLI](https://docs.protomaps.com/pmtiles/cli) (or its Docker image
`protomaps/go-pmtiles`):

```bash
pmtiles extract https://build.protomaps.com/20261010.pmtiles data/map/basemap.pmtiles \
  --bbox=-25,27,45,72 --maxzoom=12
```

Europe to zoom 12 is about 7 GB; the Upper Rhine to zoom 15 about 210 MB. `WF_BASEMAP=off`
leaves the file to you. The server sends it in byte ranges (`/api/map/basemap.pmtiles`);
OpenStreetMap is credited on the map.

## Development

Requirements: Rust (stable), Node.js 22.12 or newer, pnpm.

```bash
pnpm install
pnpm dev
```

`pnpm dev` creates `.env` from [`.env.example`](.env.example) on the first run, then starts
the server on :8080 and the web app with hot reload on :5173 (it proxies `/api`). Open
http://localhost:5173. Data goes to `./.data`.

Checks, as CI runs them:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace     # also writes the TypeScript bindings
pnpm lint                  # Biome
pnpm typecheck
pnpm test
pnpm build
```

### Layout

```
crates/
  wf-core/       models, units, stations, forecast series, measurements, TAF, gauges,
                 verification and the mix, database
  wf-ingest/     Open-Meteo, station lists, measurements (METAR, TAF, DWD, PEGELONLINE),
                 geocoding, scheduler
  wf-map/        wind map: GRIB2 (ICON-D2, ECMWF) to PNG textures, runs on disk
  wf-api/        axum routes, response cache, station pages with meta tags, embedded web app
  windfenster/   the binary: serve | migrate | import-stations | probe
migrations/      sqlx migrations (SQLite)
web/
  src/routes/    pages (TanStack Router, file-based): start, station, point
  src/components/ forecast view and table, live and water tiles, verification (comparison
                 chart, ranking, agreement), search, map (colour field, particles, pins)
  src/lib/       logic without UI: forecast table, model choice, agreement, METAR decoder, TAF
                 texts, units, formats, API
  src/ui/        shadcn/ui components, wind colour scale
  messages/      texts in German and English (Paraglide)
  public/glyphs/ map label fonts (Noto Sans, OFL)
  scripts/       builds the map's Natural Earth basemap
  src/bindings/  types generated by ts-rs; do not edit
Dockerfile       web app → binary → slim runtime image
```

### Releases

The version lives in `Cargo.toml`. **Actions → Bump version** (or a pushed tag `vX.Y.Z`) runs
the CI, then the shared Docker release from
[firsttris/workflows](https://github.com/firsttris/workflows): amd64 and arm64 to Docker Hub
(`tristanteu/windfenster`) and ghcr.io, and the GitHub release. It needs the `DOCKER_PAT`
secret. Run by hand on main, it publishes `:edge`.

### Shared types

API types are Rust structs deriving `ts_rs::TS`. `cargo test` writes them to
`web/src/bindings`, and the web app imports them from there. After changing such a struct,
run `cargo test --workspace` and commit the generated files; CI fails if they are out of
date.

## Data sources

Weather data by [Open-Meteo.com](https://open-meteo.com/) (CC BY 4.0). Open-Meteo's free API
is for non-commercial use only. Running Windfenster commercially needs your own
[Open-Meteo subscription](https://open-meteo.com/en/pricing) or a self-hosted Open-Meteo
instance. Airport names and places come from [OurAirports](https://ourairports.com/data/)
(public domain); which airports send METARs and TAFs, and the reports themselves, from the
[Aviation Weather Center](https://aviationweather.gov/) (NOAA, public domain). Wind stations and
their 10-minute values, and the ICON-D2 fields of the wind map, come from
[DWD Open Data](https://opendata.dwd.de/). Water levels from
[PEGELONLINE](https://www.pegelonline.wsv.de/) (Wasserstraßen- und Schifffahrtsverwaltung des
Bundes), credited on every page that shows them. The map's land, borders and places come from
[Natural Earth](https://www.naturalearthdata.com/) (public domain) or, where installed, from
[OpenStreetMap](https://www.openstreetmap.org/copyright) (ODbL) through Protomaps, its labels are set in Noto
Sans ([OFL](web/public/glyphs/OFL.txt), glyphs from
[protomaps/basemaps-assets](https://github.com/protomaps/basemaps-assets)). The map's
global wind comes from [ECMWF open data](https://www.ecmwf.int/en/forecasts/datasets/open-data)
(CC BY 4.0). Further sources come with the phases that use them and are listed on an
attribution page.

---

<div align="center">

⭐ Like Windfenster? A [star on GitHub](https://github.com/firsttris/windfenster) helps others find it.<br>
🐛 [Report a bug](https://github.com/firsttris/windfenster/issues/new) · 💡 [Request a feature](https://github.com/firsttris/windfenster/issues/new)

<sub>License: <a href="LICENSE">AGPL-3.0</a> · © Tristan Teufel and contributors<br>
Changed versions you pass on or run for others must offer their source code under the AGPL; a commercial license without these obligations is available via <a href="https://teufel-it.de">teufel-it.de</a>.<br>
Windfenster is not affiliated with Windfinder, Windy, Open-Meteo, the Deutscher Wetterdienst or any other weather service.</sub>

</div>
