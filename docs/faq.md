---
description: Questions about Windfenster - an open-source alternative to Windfinder and Windy, self-hosting, which models it uses, wind alarms, costs and data.
---

# FAQ

## Is there an open-source alternative to Windfinder and Windy?

Yes, Windfenster: a wind forecast with measured wind, a model ranking per station and an animated
wind map, under the AGPL, to run on your own server. It does not copy their data: everything comes
from open sources ([Data sources](data-sources.md)).

## How is Windfenster different from Windfinder and Windy?

- It shows **which model was right** at each station: every model is checked against the
  measurements every hour, and the page opens with the best one there
  ([Forecasts and models](forecasts.md#verification-and-ranking)).
- **Measured, TAF and forecast in one table**, hour by hour, so you see at a glance whether the
  forecast is holding today.
- The map blends **ICON-D2 at 2.2 km** into the global models without an edge, and at town zoom
  the particles keep the wind's colours.
- **Your gear and your spot's directions** turn the forecast into "it works from 14 to 17".
- It runs on your own server, without tracking or ads.

## Can I self-host it, for free?

Yes. One Docker container ([Installation](installation.md)), on amd64 or arm64, about 6 GB of disk
with the defaults. The free Open-Meteo API is for non-commercial use; commercially you need an
Open-Meteo subscription or your own Open-Meteo.

## Which weather models does it use?

ICON-D2, AROME and HARMONIE (1.5–2.2 km, Europe) and ECMWF IFS and GFS (worldwide) for the
forecast pages; ICON-D2, ICON global and ECMWF IFS on the map. Plus the Windfenster mix, weighted by
how well each model hit at the station.

## Does it work outside Germany?

Yes: the map and the global models cover the world, and every airport that sends METAR reports is
a station. The regional models (ICON-D2, AROME, HARMONIE) and the DWD's wind stations are
European; water gauges are German rivers.

## Can I get a wind alarm for my kite spot?

Yes. Save the spot with the wind directions that work there, enter your kites or wings, and set
the alarm: Windfenster tells you ahead from the forecast and/or when the station measures it, by
push (Android, desktop, iPhone from the home screen) and e-mail ([Alarms](accounts.md#alarms)).

## Why does the map look coarse when I zoom in?

The basemap is only as deep as the server cut it: Europe to zoom 8 by default, full detail 20 km
around stations and places people opened. Raise `WF_BASEMAP_ZOOM`, or cut your own area deeper
([The basemap and relief](basemap.md#changing-the-depth)).

## Why do I see no particles on the map?

The map needs WebGL2. On older versions, Chrome and Brave on Linux with an AMD card showed no
particles (a driver reads textures at too low precision); that is fixed from v0.1.6. With
*reduce motion* set in the system, arrows that stand still take the particles' place.

## Where does the data come from, and is it up to date?

Measurements every 10 minutes (METAR, DWD), TAFs every 30, forecasts after every new model run,
the map's fields every 10 minutes as the weather services publish them. Every source and its
licence: [Data sources](data-sources.md).
