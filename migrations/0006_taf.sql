-- Phase 2: airport forecasts (TAF) from aviationweather.gov, one row per issued TAF. An
-- amendment has its own issue time, so the newest row of a station is the one in force.
CREATE TABLE taf (
    station_id INTEGER NOT NULL REFERENCES station (id) ON DELETE CASCADE,
    issued_at  INTEGER NOT NULL,
    valid_from INTEGER NOT NULL,
    valid_to   INTEGER NOT NULL,
    -- the TAF as sent
    raw        TEXT NOT NULL,
    -- its change groups as JSON (wf_core::taf::TafGroup); the hourly series is derived from them
    groups     TEXT NOT NULL,
    fetched_at INTEGER NOT NULL,
    PRIMARY KEY (station_id, issued_at)
) WITHOUT ROWID;

CREATE INDEX taf_valid_to ON taf (valid_to);
