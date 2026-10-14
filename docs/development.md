---
description: Build and develop Windfenster - Rust, Node.js and pnpm, the checks CI runs, unit and browser tests with Playwright and SwiftShader, screenshots, documentation and releases.
---

# Development

Requirements: Rust (stable), Node.js 22.12 or newer, pnpm.

```bash
pnpm install
pnpm dev
```

`pnpm dev` creates `.env` from [`.env.example`](https://github.com/firsttris/windfenster/blob/main/.env.example) on the first run, then starts
the server on :8080 and the web app with hot reload on :5173 (it proxies `/api`). Open
http://localhost:5173. Data goes to `./.data`. `WF_DEV_UI` (set by `pnpm dev`) names the Vite
server the binary redirects to while no web app is built in; it is a development setting only.

Checks, as CI runs them:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace     # also writes the TypeScript bindings
pnpm lint                  # Biome
pnpm typecheck
pnpm test
pnpm build
cargo build -p windfenster # the server the browser tests run against
pnpm e2e                   # browser tests (Playwright): the map's shaders and the whole app
```

## Tests

Unit tests: `cargo test` (next to the code, API and pipeline tests in `crates/*/tests` against
local stand-ins for the data sources) and Vitest for the logic in `web/src/lib`.

`pnpm e2e` runs two sets of browser tests in Chromium, with SwiftShader for a GPU; it needs
Playwright's Chromium (`pnpm --filter @windfenster/web exec playwright install chromium`), or
another one in `PW_CHROMIUM`. CI runs them in the official Playwright image of the version in
the lockfile, in two halves at once (`--shard`), each in about two minutes. CI builds the Docker
image only when its own files change (`Dockerfile`, `docker/`, `Cargo.lock`, the package
manifests), by hand and on its weekly run; the image's build cooks the Rust dependencies in a
layer of their own with cargo-chef, so a code change compiles only the workspace crates. Its
build cache lives in the registry (`ghcr.io/firsttris/windfenster-buildcache:linux-amd64`) and is
shared with the release's amd64 build, so either reuses what the other compiled.

- **gpu** (`web/e2e/*.spec.ts`): the map's shaders in a test page. How the three models lie over
  one another, compared with what the readout under the pointer shows, for wind and gusts; the
  particles (spread, speed, direction, no strokes when they start anew); the colour field; and
  the look of three scenes (a low over all three models with wind and with gusts, calm air)
  against approved pictures in `web/e2e/look.spec.ts-snapshots`. A change meant to alter the
  look updates them with `pnpm e2e --update-snapshots`; look at them before committing.
- **app** (`web/e2e/app/*.spec.ts`): the whole app against the real server (the debug build, or
  `WF_BIN`), started on a fresh data directory with `WF_SCHEDULER=false` and `WF_SOURCES`
  pointing every outside source at a local stand-in (`web/e2e/app/stand-in.mjs`: Open-Meteo,
  the place search, aviationweather.gov, the DWD, OurAirports, in their own formats); the map's
  textures are written in before it starts. Nothing is downloaded. Station pages, the forecast
  table against the API, a DWD station, the search, the map (all three models, ICON global in
  pieces, a spot's hours against the API, picking an hour and a place, the model and the gust
  layer), a passkey account (Chrome's virtual authenticator, sign-in by autofill) with a saved
  spot and its alarm, and an account with e-mail and password. Any error in the browser or from
  the server fails the test it happens in.

## Screenshots and documentation

The pictures in `docs/` come from the app against the same stand-in as the browser tests
(`web/e2e/app/screenshots.spec.ts`, in English):

```bash
pnpm screenshots    # needs the server built: cargo build -p windfenster
```

The **Update screenshots** workflow takes them after every push to `main` that touches the app,
the server or the stand-in data, in the official Playwright image, and commits the ones that
changed (`github-actions[bot]`: pull before the next push). By hand: **Actions → Update
screenshots → Run workflow**.

The documentation is this site, built with [MkDocs Material](https://squidfunk.github.io/mkdocs-material/)
from `docs/` and published to GitHub Pages on every push to `main` that touches it
(`.github/workflows/docs.yml`):

```bash
pip install -r requirements-docs.txt
mkdocs serve           # http://127.0.0.1:8000, reloads on change
mkdocs build --strict  # as CI does
```

## Repository layout

```
crates/
  wf-core/       models, units, stations, forecast series, measurements, TAF, gauges,
                 verification and the mix, accounts, favourites, gear, alarms, database
  wf-ingest/     Open-Meteo, station lists, measurements (METAR, TAF, DWD, PEGELONLINE),
                 geocoding, scheduler
  wf-map/        wind map: GRIB2 (ICON-D2, ICON global, ECMWF) to PNG textures, runs on disk
  wf-alarm/      alarms: windows from the map's fields, live from measurements, push and e-mail
  wf-api/        axum routes, response cache, station pages with meta tags, embedded web app
  windfenster/   the binary: serve | migrate | import-stations | probe | admin
migrations/      sqlx migrations (SQLite)
web/
  src/routes/    pages (TanStack Router, file-based): start, station, point, map, account,
                 gear, admin, legal notice and privacy policy
  src/components/ forecast view and table, live and water tiles, verification (comparison
                 chart, ranking, agreement), search, map (colour field, particles, pins),
                 admin/ (the admin page's sections)
  src/lib/       logic without UI: forecast table, model choice, agreement, METAR decoder, TAF
                 texts, units, formats, API
  src/ui/        shadcn/ui components, wind colour scale
  messages/      texts in German and English (Paraglide)
  public/glyphs/ map label fonts (Noto Sans, OFL)
  public/sw.js   service worker that shows the alarms' push messages
  scripts/       builds the map's Natural Earth basemap
  src/bindings/  types generated by ts-rs; do not edit
Dockerfile       web app → binary → slim runtime image
compose.yaml     Docker Compose with the published image
windfenster.container  Podman Quadlet unit
```

## Releases

The version lives in `Cargo.toml`. **Actions → Bump version** (pick patch, minor or major)
raises it there and in `Cargo.lock`, commits, tags `vX.Y.Z` and starts the release; a tag pushed
from a checkout does the same for the version already in `Cargo.toml`. The release runs the CI
(skipped when main's CI already passed for the same code, as after **Bump version**), then the
shared Docker release from [firsttris/workflows](https://github.com/firsttris/workflows):
amd64 and arm64 as `:x.y.z`, `:x.y` and `:latest` to Docker Hub (`tristanteu/windfenster`) and
ghcr.io, and the GitHub release. Each platform keeps its build cache in
`ghcr.io/firsttris/windfenster-buildcache`, so a release reuses the layers of the one before. It
needs the `DOCKER_PAT` secret (a Docker Hub access token of `tristanteu`). Servers with `podman auto-update` pick up the new `:latest` within a day.
**Actions → Release**, run by hand on main, publishes `:edge` and releases nothing.

## Conventions

- Functional, not object-oriented: pure functions that take data and return data, side effects at
  the edges; errors as values (`thiserror` in the library crates).
- A file does one thing; past about 400 lines it is split.
- Code, comments, the README, the docs and commits in English; the interface in German (base) and
  English, every text in `web/messages/de.json` and `en.json`.
- A migration that is pushed is never changed; a correction is a new migration.
- Every change brings its tests, the regenerated bindings, both languages' texts and the docs it
  touches.
