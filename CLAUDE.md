# Notes for Claude

## Documentation

- The documentation is in `docs/` (MkDocs Material, <https://firsttris.github.io/windfenster/>).
  Read the pages that touch the change before starting: `architecture.md` for how the pieces fit,
  `wind-map.md` and `basemap.md` for the map, `forecasts.md` and `measurements.md` for the data.
  Links in the README and the docs point to the GitHub Pages site, not to `docs/*.md`.
- The plan, a Claude Doc in German: https://claude.ai/code/artifact/7ee0f1ac-b9dd-426a-afd7-5e8d2e919a22.
  Read the section of the phase you work on before starting, and keep it current: tick finished
  items, write down every decision that changes it (data model, sources, stack).
- The mockup, the reference for look and flow: https://claude.ai/artifact/6JLKw5J5fu7dGV8L4ZLvrx
  (station page desktop and mobile, alarm set-up, lock screen, wind map).
- `README.md` sells and gets people started (firsttris/workflows `templates/README.template.md`);
  details belong in the docs.

## Reference implementations

Base details on the sources, not on memory. Clone them shallowly (`git clone --depth 1`) into the
scratchpad and name the source file in comments and commit messages where it settles a detail.

- **firsttris/haul**: the architecture this project copies: one Rust binary with the SPA embedded
  (`crates/haul/src/ui.rs`, `main.rs`, `db.rs`), the Dockerfile, Paraglide (`ui/src/i18n.ts`).
- **Open-Meteo**: forecast and geocoding API, model ids and run metadata
  (https://open-meteo.com/en/docs, source at github.com/open-meteo/open-meteo).
- **mapbox/webgl-wind**: the particle layer of the wind map (Phase 3).
- **firsttris/namarr** `notify.server.ts` (alarm channels) and **firsttris/haben** (passkey
  autofill), both Phase 4.

## With every change

Update what the change affects, in the same commit, and only that:

- the tests for what changed: `cargo test` (unit tests next to the code, API and pipeline tests in
  `crates/*/tests` against local stand-ins for the data sources), Vitest for logic in `web/src/lib`;
- the TypeScript bindings: `cargo test` writes them to `web/src/bindings`; commit them;
- the texts in `web/messages/de.json` and `en.json`, both languages, for every text the UI or the
  station pages show;
- the docs in `docs/` that describe it, and the README when a feature a user sees changes;
- the screenshots after a change to the look: **Actions → Update screenshots → Run workflow**
  (`pnpm screenshots` locally);
- the plan when a decision or a phase item changes.

## Checks

Green before every push, then check CI:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm lint
pnpm typecheck
pnpm test
pnpm build
cargo build -p windfenster
pnpm e2e        # browser tests: map shaders and the whole app; PW_CHROMIUM=… for another Chromium
mkdocs build --strict    # when docs/ or mkdocs.yml changed
```

CI runs on the newest stable Rust, which can bring new clippy lints: run clippy with
`cargo +stable` when the local toolchain is older.

## Commits

- Work happens directly on `main` (the user's choice for this project); no pull requests unless
  asked.
- One step per commit (a phase part, the localization, a fix), with a message that says what
  changed and why.

## Working rules

- Ask before changing anything the tech stack decides (libraries, tooling, delivery).
- Commands as root (`sudo`, `pkexec`) are run by the user; don't work around that.
- The user's running processes are not stopped; use other ports for tests.
- Language: code, comments, README, docs, commits in English; the plan in German; the interface in
  German (base) and English, through Paraglide.
- Cloud sessions may not reach the data sources (Open-Meteo, DWD, aviationweather.gov,
  PEGELONLINE). Never fill in made-up data; build against the documented format, test with local
  stand-ins, and say what could not be checked against the live service.

## Code style

Technology choices follow `tech-stack.md` in firsttris/workflows. Deviations of this project and
why:

- Backend in Rust (performance, GRIB decoding, one binary, as in haul), so no TanStack Start;
  API types cross to the SPA through ts-rs instead of Zod schemas.
- Passkeys (Phase 4): webauthn-rs on the server, `@simplewebauthn/browser` in the SPA, own
  sessions instead of Better Auth (a TypeScript server library).
- Error types: the library crates still return `anyhow` in places; move them to `thiserror`
  when touching them (`anyhow` stays in the binary).

Functional, not object-oriented: pure functions that take data and return data, side effects at
the edges; function components and hooks, no classes; errors as values. A file does one thing:
split it past ~400 lines, a function past ~80. A refactor changes no behaviour.

## Project specifics

- **Migrations:** never change one that is pushed, not even a comment: sqlx checks their
  checksums and an existing database would refuse to start. Correct with a new migration.
- **Units:** the server stores and sends wind in knots; the SPA converts (kn, m/s, km/h).
- **Places:** forecasts are stored per point on the 0.01° grid; stations use the grid point of
  their coordinates (`wf_core::geo::GridPoint`).
- **API texts:** the API never sends prose. Errors and notices are codes (`ErrorCode`, `Notice`);
  the SPA maps each to a message, and a missing mapping fails the type check.
- **Open-Meteo budget:** the free API allows about 10,000 calls a day. A place is fetched when
  opened, once per model run; places opened within 7 days are refreshed per run.
  `windfenster probe --lat … --lon …` shows what Open-Meteo returns for every model.
- **Reference stations:** Baden Airpark on the Rhine (EDSB, Pegel Iffezheim) and Rhodes (LGRP).
  The Phase 1 gate is "EDSB shows 5 models".
- **Bar for the look:** better looking than Windy and Windfinder, side by side; a phase with a
  visible UI is done only then.
- Local data in `./.data`; `pnpm dev` runs server (:8080) and Vite (:5173).
