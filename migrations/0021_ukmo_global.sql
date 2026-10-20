-- UKMO Global (Met Office, 10 km, 7 days, 4 runs a day): a third global model, among the best for
-- wind, ranked after ECMWF IFS. Lead time and runs from Open-Meteo's
-- Sources/App/UKMO/UkmoDomain.swift (168 hours from 00 and 12 UTC, 60 from 06 and 18 UTC, every
-- 6 hours) and its docs page (0.09°, about 10 km). It ends before the page's ten days, so it is
-- never the global model that fills them.
INSERT INTO model (id, name, provider, resolution_km, class, bbox_west, bbox_south, bbox_east, bbox_north,
                   max_lead_h, runs_per_day, rank, meta_id)
VALUES ('ukmo_global_deterministic_10km', 'UKMO Global', 'Met Office', 10.0, 'global', NULL, NULL, NULL, NULL,
        168, 4, 43, 'ukmo_global_deterministic_10km');
