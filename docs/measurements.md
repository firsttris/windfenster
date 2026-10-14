---
description: Where Windfenster's measured wind comes from - METAR reports of every airport, DWD 10-minute stations, airport forecasts (TAF), PEGELONLINE water levels - and how the history and the ten-year wind statistics are made.
---

# Measurements, TAF and gauges

The forecast is only half of it: Windfenster puts what was **measured** next to it, at every
airport that sends METAR reports and every DWD wind station, plus the airport forecast (TAF) and
water levels on German rivers.

## METAR and DWD stations

Every ten minutes the server reads aviationweather.gov's cache of all current METARs (one file of
about 250 KB) and keeps the reports of its stations: every airfield in the world that sends
METARs, from OurAirports' list and aviationweather.gov's station list, refreshed weekly.

DWD stations deliver **10-minute values**. They are fetched every 30 minutes for the tracked
stations (`WF_TRACKED_STATIONS`, or set on the [admin page](admin.md)) and those opened within the last seven days, and right away when
someone opens a station whose values are stale.

The live tile shows the newest report, and for an airport the raw METAR translated group by group
(wind, visibility, weather, clouds, temperature, pressure), decoded in the browser in German and
English.

### How long measurements are kept

Measurements and TAFs are kept for **13 months** (for the verification of the models) at tracked
stations, stations opened within the last 30 days and anyone's favourites; all other stations keep
11 days, the reach of the history chart. The reports' raw text goes after 2 days. Cleaning up runs
station by station, so the database is never locked for long.

## TAF

TAFs (airport forecasts written by meteorologists for 24 to 30 hours) come from aviationweather.gov's
TAF API every 30 minutes, with the change groups already decoded. Windfenster turns them into an
**hourly series**:

| Group | Becomes |
|---|---|
| base, `FM` (from) | the prevailing wind from then on |
| `BECMG` (becoming) | the new wind from the middle of the change; the span of both during it |
| `TEMPO` (at times), `PROB30/40` | a span beside the prevailing wind, e.g. `12–18` |

The TAF row in the day cards shows that series; the notes under the card say in words what the
groups add for the day (showers, a temporary gust, low clouds). The TAF is also verified like a
model.

## The history chart

Each station page shows its measurements over a day, as Windfinder does: the last 24 hours and
the ten days before, with mean wind, gusts and direction, and the value under the pointer. When a
station's past days are thin, the server fills them in once: the METAR API keeps about eight days,
the DWD's archive of 10-minute values about a year and a half.

## Wind statistics

A station page shows the station's wind over the last ten whole years: per month a wind rose
(16 directions, the wind scale's bands) and the share of windy days, those with at least two
daylight hours (9 to 19 by the sun) of 12 kn or more mean wind. The hours come from an archive
on the first request, one station at a time, in the background: airports from the
[Iowa Environmental Mesonet](https://mesonet.agron.iastate.edu/request/download.phtml)'s METAR
archive (routine reports, a year per request, a pause between requests as it asks), DWD
stations from the DWD's climate archive of hourly wind. Only the summary is kept (a few KB a
station), and made anew once another year has ended. `GET /api/stations/{slug}/stats` answers
`pending` until it is ready.

## Water gauges (PEGELONLINE)

Water gauges come from [PEGELONLINE](https://www.pegelonline.wsv.de/): one request every 10
minutes brings all of them (about 730 with coordinates) with level, flow, water temperature and
the official flood states. A station or point within 15 km of a gauge shows it; the map shows all
of them from zoom 8.5, as a dot in the colour of its state (blue normal, amber low, red high, grey
without a fresh reading). The last seven days are fetched from PEGELONLINE when the page is
opened.

The **current on the Rhine** is estimated from the flow and a blanket cross-section (300 m × 6 m,
surface speed 1.3 times the mean) and labelled as an estimate.
