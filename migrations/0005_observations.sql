-- Phase 2: measurements.

-- The station publishes a TAF (aviationweather.gov).
ALTER TABLE station ADD COLUMN has_taf INTEGER NOT NULL DEFAULT 0;
-- Kept current all the time: forecasts after every run, measurements, verification. Set from
-- WF_TRACKED_STATIONS at start; a station someone opened is kept current for 7 days anyway.
ALTER TABLE station ADD COLUMN tracked INTEGER NOT NULL DEFAULT 0;

-- Measured wind, from METAR (all stations, every 10 minutes) or the DWD (tracked stations).
CREATE TABLE observation (
    station_id  INTEGER NOT NULL REFERENCES station (id) ON DELETE CASCADE,
    observed_at INTEGER NOT NULL,
    wind_kn     REAL,
    gust_kn     REAL,
    -- where the wind comes from, degrees; NULL when variable
    dir_deg     REAL,
    temp_c      REAL,
    -- the report as sent (METAR); NULL for DWD values
    raw         TEXT,
    PRIMARY KEY (station_id, observed_at)
) WITHOUT ROWID;
