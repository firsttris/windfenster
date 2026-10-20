-- ICON-CH1 (1 km, 33 h, 8 runs a day) and ICON-CH2 (2 km, 120 h, 4 runs a day) of MeteoSwiss:
-- Superforecast for the Alps, where valley winds, thermals and foehn need the finest grid
-- (Swiss lakes, Lake Constance, Lake Garda's north). Both on Open-Meteo's rotated grid
-- (Sources/App/MeteoSwiss/MeteoSwissDomain.swift: pole 43° N 190° E, 1089 × 705 points at
-- 0.01° and 545 × 353 at 0.02° from -6.46°, -4.06°); the domain below is that grid's outline in
-- latitude and longitude, rounded outwards, so its corners lie outside the model and answer
-- without values. Lead times and runs from the same file and its docs page. CH1 is ranked
-- right after ICON-D2, CH2 after AROME.
INSERT INTO model (id, name, provider, resolution_km, class, bbox_west, bbox_south, bbox_east, bbox_north,
                   max_lead_h, runs_per_day, rank, meta_id)
VALUES ('meteoswiss_icon_ch1', 'ICON-CH1', 'MeteoSwiss', 1.0, 'super', 0.02, 42.57, 16.85, 49.99, 33, 8, 12,
        'meteoswiss_icon_ch1'),
       ('meteoswiss_icon_ch2', 'ICON-CH2', 'MeteoSwiss', 2.0, 'super', 0.02, 42.57, 16.85, 49.99, 120, 4, 25,
        'meteoswiss_icon_ch2');
