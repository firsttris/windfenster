-- ICON-EU (DWD, 7 km, 5 days, 8 runs a day): the regional model for the whole Mediterranean,
-- where the finer ones stop (Rhodes lies south of ICON-D2, AROME and HARMONIE). At 7 km it is no
-- Superforecast model (≤ 3 km), so it joins the Forecast class with its domain, ranked after
-- ECMWF IFS. Domain from Open-Meteo's grid (Sources/App/Icon/Icon.swift: 1377 × 657 points at
-- 0.0625° from 23.5° W, 29.5° N), lead time and runs from IconDownloadDomains.swift and its
-- docs (120 h from 00/06/12/18 UTC, every 3 hours).
INSERT INTO model (id, name, provider, resolution_km, class, bbox_west, bbox_south, bbox_east, bbox_north,
                   max_lead_h, runs_per_day, rank, meta_id)
VALUES ('icon_eu', 'ICON-EU', 'DWD', 7.0, 'global', -23.5, 29.5, 62.5, 70.5, 120, 8, 46, 'dwd_icon_eu');
