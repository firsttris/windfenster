-- Phase 1: station details for search and pages, places, the first five models.

-- Region and town for breadcrumbs and search, e.g. 'Baden-Württemberg', 'Rheinmünster'.
ALTER TABLE station ADD COLUMN region TEXT;
ALTER TABLE station ADD COLUMN municipality TEXT;
-- name, code, town and region in lower case without accents; searched with LIKE
ALTER TABLE station ADD COLUMN search TEXT NOT NULL DEFAULT '';
-- last import that still listed the station
ALTER TABLE station ADD COLUMN seen_at INTEGER NOT NULL DEFAULT 0;

CREATE INDEX station_lat_lon ON station (lat, lon);

-- A place forecasts are fetched for: the grid point of a station or of a free point (0.01°).
-- Places asked for recently are "hot" and refreshed after every model run.
CREATE TABLE place (
    lat_e2       INTEGER NOT NULL,
    lon_e2       INTEGER NOT NULL,
    -- IANA time zone and model elevation, as Open-Meteo reports them for the point
    timezone     TEXT,
    elevation_m  REAL,
    requested_at INTEGER NOT NULL,
    -- 1 for the grid point of a station; free points are dropped 2 days after the last request
    pinned       INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (lat_e2, lon_e2)
) WITHOUT ROWID;

-- Default order while a station has no verification yet (lower first).
ALTER TABLE model ADD COLUMN rank INTEGER NOT NULL DEFAULT 100;
-- Open-Meteo's dataset name for /data/{meta_id}/static/meta.json, which reports the newest run.
ALTER TABLE model ADD COLUMN meta_id TEXT;

-- The five models of v1. Ids, resolutions, domains and horizons follow Open-Meteo's
-- documentation; they still have to be checked against the live API (`windfenster probe`).
-- A domain only decides which models are requested for a point: a model that returns no
-- values there counts as not covering it.
INSERT INTO model (id, name, provider, resolution_km, class, bbox_west, bbox_south, bbox_east, bbox_north,
                   max_lead_h, runs_per_day, rank, meta_id)
VALUES
    ('icon_d2', 'ICON-D2', 'DWD', 2.2, 'super', -3.94, 43.18, 20.34, 58.08, 48, 8, 10, 'dwd_icon_d2'),
    ('meteofrance_arome_france_hd', 'AROME', 'Météo-France', 1.5, 'super', -12.0, 37.5, 16.0, 55.4, 42, 8, 20,
     'meteofrance_arome_france_hd'),
    ('dmi_harmonie_arome_europe', 'HARMONIE', 'DMI', 2.0, 'super', -25.0, 39.0, 40.0, 72.0, 60, 8, 30,
     'dmi_harmonie_arome_europe'),
    ('ecmwf_ifs025', 'ECMWF IFS', 'ECMWF', 25.0, 'global', NULL, NULL, NULL, NULL, 360, 4, 40, 'ecmwf_ifs025'),
    ('gfs_global', 'GFS', 'NOAA', 13.0, 'global', NULL, NULL, NULL, NULL, 384, 4, 50, 'ncep_gfs013');
