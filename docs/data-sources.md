---
description: The open data behind Windfenster - Open-Meteo, DWD Open Data, ECMWF open data, aviationweather.gov, OurAirports, PEGELONLINE, OpenStreetMap via Protomaps, Mapterhorn - and their licences.
---

# Data sources

Weather data by [Open-Meteo.com](https://open-meteo.com/) (CC BY 4.0). Open-Meteo's free API
is for non-commercial use only. Running Windfenster commercially needs your own
[Open-Meteo subscription](https://open-meteo.com/en/pricing) or a self-hosted Open-Meteo
instance. Airport names and places come from [OurAirports](https://ourairports.com/data/)
(public domain); which airports send METARs and TAFs, and the reports themselves, from the
[Aviation Weather Center](https://aviationweather.gov/) (NOAA, public domain). Wind stations and
their 10-minute values, and the ICON-D2 fields of the wind map, come from
[DWD Open Data](https://opendata.dwd.de/) (CC BY 4.0, "Datenbasis: Deutscher Wetterdienst"), as do the DWD stations' past years for
the wind statistics; the airports' past years come from the
[Iowa Environmental Mesonet](https://mesonet.agron.iastate.edu/) (Iowa State University). Water
levels from [PEGELONLINE](https://www.pegelonline.wsv.de/) (Wasserstraßen- und Schifffahrtsverwaltung des
Bundes), credited on the station and point pages that show them and on the sources page; the map
leaves it out of its short credit line. The map's land, borders and places come from
[Natural Earth](https://www.naturalearthdata.com/) (public domain) or, where installed, from
[OpenStreetMap](https://www.openstreetmap.org/copyright) (ODbL) through Protomaps, its labels are set in Noto
Sans ([OFL](https://github.com/firsttris/windfenster/blob/main/web/public/glyphs/OFL.txt), glyphs from
[protomaps/basemaps-assets](https://github.com/protomaps/basemaps-assets)). The map's
global wind comes from [ECMWF open data](https://www.ecmwf.int/en/forecasts/datasets/open-data)
(CC BY 4.0), its relief from [Mapterhorn](https://mapterhorn.com/attribution) (open elevation
data of many national surveys, mostly CC BY; credited on the map). The app lists every source with
what it is used for, its licence and the credit it asks for on its sources page (`/sources`,
linked from the footer and the map).

| Source | Used for | Licence |
|---|---|---|
| [Open-Meteo](https://open-meteo.com/) | point forecasts of every model, place search | CC BY 4.0; free API non-commercial |
| [DWD Open Data](https://opendata.dwd.de/) | 10-minute wind stations, ICON-D2 and ICON global fields, station archives | CC BY 4.0 |
| [ECMWF open data](https://www.ecmwf.int/en/forecasts/datasets/open-data) | the global wind fields (from its Google Cloud copy when data.ecmwf.int is busy) | CC BY 4.0 |
| [Aviation Weather Center](https://aviationweather.gov/) | METAR and TAF reports, station list | public domain |
| [OurAirports](https://ourairports.com/data/) | airport names and places | public domain |
| [Iowa Environmental Mesonet](https://mesonet.agron.iastate.edu/) | airports' past years for the statistics | free use, credited |
| [PEGELONLINE](https://www.pegelonline.wsv.de/) | water levels, flow, flood states | open data of the WSV |
| [OpenStreetMap](https://www.openstreetmap.org/copyright) via [Protomaps](https://protomaps.com/) | the detailed basemap | ODbL |
| [Natural Earth](https://www.naturalearthdata.com/) | the basemap without a detailed one | public domain |
| [Mapterhorn](https://mapterhorn.com/attribution) | relief | mostly CC BY, per survey |

!!! warning "Commercial use"
    Open-Meteo's free API is for non-commercial use only. Running Windfenster commercially needs
    your own [Open-Meteo subscription](https://open-meteo.com/en/pricing) or a self-hosted
    Open-Meteo instance (`OPEN_METEO_API_KEY`, `OPEN_METEO_BASE_URL`).
