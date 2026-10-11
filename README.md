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
> (Phase 1). Phase 2 is under way: live measurements from METAR and DWD stations and the
> airport forecasts (TAF) are in; water levels and the model ranking follow.

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
| `WF_DATA_DIR` | `/data` | Database (later also the map textures) |
| `WF_LISTEN` | `0.0.0.0:8080` | Address of the HTTP server |
| `WF_PUBLIC_URL` | | Public address, e.g. `https://windfenster.example`, for canonical links of station pages |
| `OPEN_METEO_API_KEY` | | Open-Meteo key; without one the free, non-commercial API is used |
| `OPEN_METEO_BASE_URL` | | Own Open-Meteo instance |
| `OPEN_METEO_GEOCODING_URL` | | Own Open-Meteo geocoding instance (place search) |
| `WF_OPEN_METEO_DAILY_BUDGET` | 9,000 without key | Open-Meteo calls per day |
| `WF_MODELS` | all | Models to use, comma-separated Open-Meteo ids |
| `WF_TRACKED_STATIONS` | `edsb,lgrp` | Stations (slugs) kept current and verified even when nobody opens them |
| `WF_SCHEDULER` | `true` | Background jobs: station lists weekly, model runs every 5 minutes, METARs every 10, TAFs and DWD measurements every 30 |
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
  wf-core/       models, units, stations, forecast-series format, database
  wf-ingest/     Open-Meteo, station lists, measurements (METAR, DWD), geocoding, scheduler
  wf-api/        axum routes, response cache, station pages with meta tags, embedded web app
  windfenster/   the binary: serve | migrate | import-stations | probe
migrations/      sqlx migrations (SQLite)
web/
  src/routes/    pages (TanStack Router, file-based): start, station, point
  src/components/ forecast view and table, live tile, search
  src/lib/       logic without UI: forecast table, METAR decoder, TAF texts, units, formats, API
  src/ui/        shadcn/ui components, wind colour scale
  messages/      texts in German and English (Paraglide)
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
their 10-minute values come from [DWD Open Data](https://opendata.dwd.de/). Further sources
(PEGELONLINE, ECMWF) come with the phases that use them and are listed on an attribution page.

---

<div align="center">

⭐ Like Windfenster? A [star on GitHub](https://github.com/firsttris/windfenster) helps others find it.<br>
🐛 [Report a bug](https://github.com/firsttris/windfenster/issues/new) · 💡 [Request a feature](https://github.com/firsttris/windfenster/issues/new)

<sub>License: <a href="LICENSE">AGPL-3.0</a> · © Tristan Teufel and contributors<br>
Changed versions you pass on or run for others must offer their source code under the AGPL; a commercial license without these obligations is available via <a href="https://teufel-it.de">teufel-it.de</a>.<br>
Windfenster is not affiliated with Windfinder, Windy, Open-Meteo, the Deutscher Wetterdienst or any other weather service.</sub>

</div>
