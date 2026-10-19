---
description: Windfenster's JSON API - stations, points, forecasts, live measurements, history, statistics, water gauges, the wind map's data, accounts, spots, gear and alarms.
---

# API

The web app uses a JSON API under `/api`; you can use it too. Wind is always in **knots**, times
are **Unix seconds**, places are on the 0.01° grid. The types are Rust structs exported to
TypeScript in [`web/src/bindings`](https://github.com/firsttris/windfenster/tree/main/web/src/bindings):
that folder is the exact schema of every answer.

The API never sends prose. An error is an HTTP status and a code, e.g.

```json
{ "code": "invalid_coordinates" }
```

(`not_found`, `invalid_coordinates`, `invalid_range`, `signed_out`, `too_many_favorites`, …; see
`ErrorCode.ts`), and forecasts carry a `notice` code where something is missing.

## Forecasts and stations

| Method and path | Gives |
|---|---|
| `GET /api/health` | version, schema version and number of stations |
| `GET /api/models` | the models with resolution, area, reach and runs |
| `GET /api/stations?q=…&lang=de` | search: stations by name or code, places by name, coordinates |
| `GET /api/stations/{slug}` | a station with its forecast of every model and its ranking |
| `GET /api/point?lat=…&lon=…` | the forecast for any point, the nearest station and its ranking |
| `GET /api/stations/{slug}/live` | the latest measurements and the TAF |
| `GET /api/stations/{slug}/history?from=…&to=…` | measurements over a span (the history chart) |
| `GET /api/stations/{slug}/stats` | the ten-year wind statistics, or `pending` while they are made |
| `GET /api/gauges/near?lat=…&lon=…` | the water gauge within 15 km, with its last seven days |

```bash
curl -s 'http://localhost:8080/api/point?lat=48.78&lon=8.08' | jq '.forecast.models[].name'
```

## The wind map

| Method and path | Gives |
|---|---|
| `GET /api/map` | the hours on offer per model, their areas, the basemap's and relief's extent |
| `GET /api/map/{model}/{run}/{step}.png`, `.json` | one hour's texture and its meta |
| `GET /api/map/{model}/{run}/{step}/{w}_{s}_{e}_{n}.png`, `.json` | a piece of ICON global |
| `GET /api/map/series?lat=…&lon=…` | wind, gusts and direction at a place over every hour |
| `GET /api/map/stations` | station pins with their latest measured wind |
| `GET /api/map/gauges` | gauge pins with their level and state |
| `GET /api/map/basemap/{z}/{x}/{y}` | a basemap vector tile (MVT) |
| `GET /api/map/basemap.pmtiles`, `/api/map/terrain.pmtiles` | the basemap and relief archives, by byte range |

How these are made: [The wind map](wind-map.md#the-api).

## Accounts

Signed-in requests carry the session cookie the sign-in sets (HTTP-only, half a year).

| Method and path | Does |
|---|---|
| `POST /api/auth/signup`, `/signup/finish` | sign up with a passkey (name, then the WebAuthn ceremony) |
| `POST /api/auth/signup/password` | sign up with e-mail and password |
| `POST /api/auth/signin`, `/signin/finish` | sign in with a passkey |
| `POST /api/auth/signin/password` | sign in with e-mail and password |
| `POST /api/auth/passkeys`, `/passkeys/finish` | add another passkey |
| `GET /api/me/passkeys` | the account's passkeys: id, own name, browser and system, added, last used |
| `PATCH /api/me/passkeys/{id}` | name a passkey (`{"name": …}`; empty or null goes back to the browser and system) |
| `DELETE /api/me/passkeys/{id}` | remove a passkey; refused (`last_sign_in`) for the last one of an account without a password |
| `DELETE /api/me/sessions` | sign out on every other device; how many sessions ended |
| `POST /api/auth/signout` | sign out |
| `GET`, `PATCH`, `DELETE /api/me` | the account: read, rename, delete |
| `PUT /api/me/password` | set or change the password |

## Spots, gear and alarms

| Method and path | Does |
|---|---|
| `GET`, `POST /api/favorites` | list the spots, add one |
| `PUT`, `DELETE /api/favorites/{id}` | change a spot (name, wind directions), remove it |
| `PUT /api/favorites/order` | reorder |
| `GET`, `PUT /api/gear` | the whole gear list (at most 30 pieces) |
| `GET /api/alarms` | every alarm |
| `PUT`, `DELETE /api/alarms/{favorite}` | set or remove a spot's alarm |
| `POST /api/alarms/test` | send a test message |
| `GET /api/push` | the server's push key, whether e-mail is set up |
| `POST`, `DELETE /api/push/subscriptions` | subscribe a browser to push, or unsubscribe |

## The site and the admin page

| Method and path | Gives or does |
|---|---|
| `GET /api/site` | whether accounts can be made (`open`, `invite`, `closed`), the notice, whether there is a legal notice and a privacy policy |
| `GET /api/site/imprint`, `/api/site/privacy` | their text |

Under `/api/admin` the admin page's requests answer admins only (`403 forbidden` else):
`overview`, `tasks/{id}/run`, `settings` and `settings/{key}`, `map` (with `renew`, `cancel`,
`postpone`, `spots`, `spots/pinned`, `leftovers`), `forecasts`, `measurements`,
`stats/{station}/retry`, `users` (with `role`, `disable`, `enable`, `signout`, `export`,
`confirm-email`), `invites`, `alarms` and `alarms/test-mail`, `storage` (with `vacuum`, `prune`,
`backup`, `push-key`), `system` and `system/caches/clear`, `log` and `log/lines`. They change with
the admin page; see [The admin page](admin.md).

## Pages

`GET /station/{slug}` is the app's page with the station's title, description, Open Graph tags and
JSON-LD filled in for search engines and link previews. Every other path serves the app.
