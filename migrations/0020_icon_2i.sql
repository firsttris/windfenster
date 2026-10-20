-- ICON-2I (ItaliaMeteo-ARPAE, 2.2 km, 3 days, 2 runs a day): Superforecast for Italy and
-- around, Lake Garda, Sardinia, Sicily and the Adriatic, ranked after ICON-D2, which also covers
-- the north. Domain from Open-Meteo's grid (Sources/App/ItaliaMeteoArpae/ItaliaMeteoArpaeDomain.swift:
-- 761 × 761 points from 3° E, 33.7° N at 0.025° × 0.02°), runs every 12 hours there, 72 hours
-- ahead as its docs say.
INSERT INTO model (id, name, provider, resolution_km, class, bbox_west, bbox_south, bbox_east, bbox_north,
                   max_lead_h, runs_per_day, rank, meta_id)
VALUES ('italia_meteo_arpae_icon_2i', 'ICON-2I', 'ItaliaMeteo', 2.2, 'super', 3.0, 33.7, 22.0, 48.9, 72, 2, 15,
        'italia_meteo_arpae_icon_2i');
