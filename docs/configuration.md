---
description: All settings of Windfenster as environment variables, from the data folder and the public address to the map's zoom levels, alarms by e-mail and Open-Meteo.
---

# Configuration

Windfenster reads its settings from environment variables: `environment:` in `compose.yaml`,
`Environment=` lines in the Podman Quadlet unit, or `.env` when developing. Nothing needs setting
to start; set `WF_PUBLIC_URL` before people sign up.

## All variables

| Variable | Default | |
|---|---|---|
| `WF_DATA_DIR` | `/data` | Database and map textures |
| `WF_LISTEN` | `0.0.0.0:8080` | Address of the HTTP server |
| `WF_PUBLIC_URL` | | Public address, e.g. `https://windfenster.example`, for canonical links of station pages and the passkeys' domain; without it passkeys work on `http://localhost:8080` only |
| `WF_UID`, `WF_GID` | 10001 (Docker) | User the server runs as in the container; the data folder is handed to it on start. Under rootless Podman only when set ([Without root](installation.md#without-root)) |
| `WF_CLIENT_IP_HEADER` | `CF-Connecting-IP` | Header the proxy in front puts the client's address in, for the per-client limits: `CF-Connecting-IP` behind Cloudflare Tunnel, `X-Forwarded-For` behind Caddy or nginx open to the internet (its last entry counts), `none` for the connection's address |
| `OPEN_METEO_API_KEY` | | Open-Meteo key; without one the free, non-commercial API is used |
| `OPEN_METEO_BASE_URL` | | Own Open-Meteo instance |
| `OPEN_METEO_GEOCODING_URL` | | Own Open-Meteo geocoding instance (place search) |
| `WF_OPEN_METEO_DAILY_BUDGET` | 9,000 without key | Open-Meteo calls per day |
| `WF_MODELS` | all | Models to use, comma-separated Open-Meteo ids |
| `WF_TRACKED_STATIONS` | `edsb,lgrp` | Stations (slugs) kept current and verified even when nobody opens them |
| `WF_SCHEDULER` | `true` | Background jobs: station lists weekly, model runs every 5 minutes, METARs and water gauges every 10, TAFs and DWD measurements every 30, verification hourly |
| `WF_SOURCES` | – | For tests: every outside source (Open-Meteo, aviationweather.gov, the DWD, PEGELONLINE, OurAirports, Protomaps, Mapterhorn) under this one address, as the browser tests' stand-in serves them |
| `WF_MAP` | `true` | The wind map: ICON-D2 and ICON global from the DWD and ECMWF's IFS, checked every 10 minutes, see below; `false` turns it off |
| `WF_ICON_GLOBAL` | `true` | ICON global on the map (13 km, hourly to +78 h, to +120 h): about 1.1 GB to fetch per run (00 and 12 UTC) and 1.5 GB on disk; `false` leaves the world to ECMWF |
| `WF_BASEMAP` | `auto` | The map's detailed basemap and relief: `auto` makes them from Protomaps' daily build and Mapterhorn's elevation, unless you put a basemap in place; `off` leaves it to you |
| `WF_BASEMAP_WORLD_ZOOM` | `7` | How deep the basemap covers the whole world (4 to 12): 6 is 45 MB, 7 about 150 MB, every level roughly three to four times more |
| `WF_BASEMAP_REGION` | `-25,27,45,72` | The region covered deeper, `west,south,east,north` in degrees (Europe), or `none` |
| `WF_BASEMAP_ZOOM` | `8` | How deep the basemap covers the region (6 to 15): 8 is about 190 MB for Europe, every level roughly four times more (12 about 7 GB); from 14 on every place there has full detail and no spots are made there |
| `WF_BASEMAP_SPOTS` | `60` | Spots with detail to zoom 14 kept at most (where people look), 2 to 17 MB each; `0` makes none |
| `WF_SMTP_URL` | | SMTP server for alarm e-mails: `smtps://user:password@host:465` (TLS) or `smtp://user:password@host:587?tls=required` (STARTTLS); an `@` in the user name is written `%40`. Without it alarms go by push only |
| `WF_MAIL_FROM` | | Sender of alarm e-mails, e.g. `Windfenster <alarm@example.org>`; needed with `WF_SMTP_URL` |
| `WF_TERRAIN_ZOOM` | `7` | How deep the relief goes, the whole world (1 to 12): 6 is 278 MB, 7 is 948 MB, every level roughly three to four times more; `0` turns it off |
| `RUST_LOG` | `info,sqlx=warn` | Log level |

## Common setups

=== "Small (a Raspberry Pi)"

    ```ini
    WF_PUBLIC_URL=https://wind.example
    WF_ICON_GLOBAL=false        # ECMWF for the world: 2.2 GB less to download a day
    WF_TERRAIN_ZOOM=6           # relief 278 MB instead of 948 MB
    ```

=== "Your own area in full detail"

    ```ini
    WF_BASEMAP_REGION=7.5,47.5,9.0,49.5   # the Upper Rhine: west,south,east,north
    WF_BASEMAP_ZOOM=15                    # every street and building there, about 210 MB
    ```

=== "A large server"

    ```ini
    WF_BASEMAP_WORLD_ZOOM=8
    WF_BASEMAP_ZOOM=12      # Europe with every town's streets, about 7 GB
    WF_BASEMAP_SPOTS=500
    WF_TERRAIN_ZOOM=8
    ```

Changing a zoom or the region makes the server cut the basemap or relief again a minute after the
next start; see [The basemap and relief](basemap.md#changing-the-depth).

## Open-Meteo: free or with a key

Forecasts per place come from Open-Meteo. Without a key Windfenster uses the free API, which is
for **non-commercial use** and allows about 10,000 calls a day; Windfenster keeps to
`WF_OPEN_METEO_DAILY_BUDGET` (9,000 by default), counted per UTC day in the database, so a
restart does not start the count again. That covers a few hundred places people look at
regularly. So that nobody can use it up by opening ever new places, a client (by its address,
see `WF_CLIENT_IP_HEADER`) may open 30 places an hour and 100 a day that the server has to fetch,
and such openings use at most two thirds of the budget; the last third stays for keeping the
places people use current. Place searches go to the geocoder 300 times an hour per client;
beyond, the search finds stations only. Larger or commercial instances need an
[Open-Meteo subscription](https://open-meteo.com/en/pricing) (`OPEN_METEO_API_KEY`) or their own
Open-Meteo (`OPEN_METEO_BASE_URL`). How the calls are spent: [Forecasts and models](forecasts.md#how-forecasts-are-fetched).

## E-mail for alarms

Alarms go by push without anything to set. For e-mail, set `WF_SMTP_URL` and `WF_MAIL_FROM`:

```ini
# TLS on port 465
WF_SMTP_URL=smtps://alarm%40example.org:PASSWORD@smtp.example.org:465
# or STARTTLS on port 587
WF_SMTP_URL=smtp://alarm%40example.org:PASSWORD@smtp.example.org:587?tls=required
WF_MAIL_FROM=Windfenster <alarm@example.org>
```

An `@` in the user name is written `%40`.

Alarm e-mails go only to an address its owner confirmed: setting an address sends a link
(valid 24 hours) to `WF_PUBLIC_URL`'s `/confirm-email`, so confirming needs `WF_PUBLIC_URL` too.
At most one such mail per address in ten minutes and 200 a day per server. Addresses stored
before this check existed (up to version 0.1.8) count as confirmed.

## The binary's commands

- `windfenster serve` (default): migrates the database, serves the API and the web app, keeps
  station lists and forecasts current
- `windfenster migrate`: migrates and exits
- `windfenster import-stations`: fetches the station lists and the water gauges now
- `windfenster probe --lat 48.78 --lon 8.08`: asks Open-Meteo for every model at a point and
  prints what comes back

In the container: `docker compose exec windfenster windfenster probe --lat 48.78 --lon 8.08`.
