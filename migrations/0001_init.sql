-- Phase 0: stations, models, model runs and forecast series.
-- Times are unix seconds (UTC). Wind is stored in knots.

-- Measuring stations, imported from the sources (METAR, DWD, OpenWindMap). One page each.
CREATE TABLE station (
    id              INTEGER PRIMARY KEY,
    -- URL key of the station page (/station/{slug}), e.g. 'edsb' or 'dwd-04177'
    slug            TEXT NOT NULL UNIQUE,
    source          TEXT NOT NULL CHECK (source IN ('metar', 'dwd', 'owm')),
    -- the source's own id: ICAO code, DWD station number, OpenWindMap id
    code            TEXT NOT NULL,
    name            TEXT NOT NULL,
    -- ISO 3166-1 alpha-2
    country         TEXT,
    lat             REAL NOT NULL CHECK (lat BETWEEN -90 AND 90),
    lon             REAL NOT NULL CHECK (lon BETWEEN -180 AND 180),
    elevation_m     REAL,
    -- anemometer height above ground
    sensor_height_m REAL,
    UNIQUE (source, code)
);

-- Weather models. Static: rows are added and changed by migrations only.
CREATE TABLE model (
    -- Open-Meteo's model id, e.g. 'icon_d2'
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    provider      TEXT NOT NULL,
    resolution_km REAL NOT NULL CHECK (resolution_km > 0),
    -- 'super': regional, ≤ 3 km, picked by Superforecast; 'global': picked by Forecast
    class         TEXT NOT NULL CHECK (class IN ('super', 'global')),
    -- computational domain in degrees; all NULL for global models
    bbox_west     REAL,
    bbox_south    REAL,
    bbox_east     REAL,
    bbox_north    REAL,
    max_lead_h    INTEGER NOT NULL CHECK (max_lead_h > 0),
    runs_per_day  INTEGER NOT NULL CHECK (runs_per_day > 0),
    CHECK (
        (bbox_west IS NULL AND bbox_south IS NULL AND bbox_east IS NULL AND bbox_north IS NULL)
        OR (bbox_west < bbox_east AND bbox_south < bbox_north)
    )
);

-- One run of a model ("Lauf 12 UTC, vor 38 min").
CREATE TABLE model_run (
    id          INTEGER PRIMARY KEY,
    model_id    TEXT NOT NULL REFERENCES model (id),
    -- initialisation time of the run
    run_at      INTEGER NOT NULL,
    -- when Windfenster finished fetching it
    ingested_at INTEGER,
    status      TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'complete', 'failed')),
    UNIQUE (model_id, run_at),
    -- parent key for forecast_series (run, model)
    UNIQUE (id, model_id)
);

-- One forecast series per place, model and run (not one row per hour).
-- A place is a point on the 0.01° grid; stations use the grid point of their coordinates.
-- Each quantity is an array of little-endian i16, see wf_core::series for scales.
CREATE TABLE forecast_series (
    -- latitude and longitude × 100, rounded
    lat_e2     INTEGER NOT NULL,
    lon_e2     INTEGER NOT NULL,
    model_id   TEXT NOT NULL,
    run_id     INTEGER NOT NULL,
    -- time of the first step and the step length
    start_at   INTEGER NOT NULL,
    step_s     INTEGER NOT NULL CHECK (step_s > 0),
    wind       BLOB NOT NULL,
    gust       BLOB NOT NULL,
    dir        BLOB NOT NULL,
    temp       BLOB NOT NULL,
    rain       BLOB NOT NULL,
    cloud      BLOB NOT NULL,
    fetched_at INTEGER NOT NULL,
    PRIMARY KEY (lat_e2, lon_e2, model_id, run_id),
    FOREIGN KEY (run_id, model_id) REFERENCES model_run (id, model_id) ON DELETE CASCADE
) WITHOUT ROWID;

CREATE INDEX forecast_series_run ON forecast_series (run_id, model_id);
