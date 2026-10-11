-- Phase 2: verification. Per station, model and UTC day the errors of every forecast hour
-- that has a measurement, by lead time. Sums, so 30 days add up exactly. `model_id` is a
-- model id, 'taf' (the airport forecast) or 'mix' (the Windfenster mix); neither of the
-- last two is a row in `model`, so there is no foreign key.
CREATE TABLE verification_daily (
    station_id   INTEGER NOT NULL REFERENCES station (id) ON DELETE CASCADE,
    model_id     TEXT NOT NULL,
    -- UTC date of the verified hours, yyyymmdd
    day          INTEGER NOT NULL,
    -- lead time: 0 = 0–24 h, 1 = 24–48 h, 2 = 48–72 h after the run (TAF: after issue)
    bucket       INTEGER NOT NULL CHECK (bucket BETWEEN 0 AND 2),
    -- hours compared
    n            INTEGER NOT NULL,
    -- Σ |forecast − measured| and Σ (forecast − measured) of the mean wind, kn
    sum_abs      REAL NOT NULL,
    sum_err      REAL NOT NULL,
    -- direction: hours with at least 5 kn measured, Σ of the angle between, degrees
    n_dir        INTEGER NOT NULL,
    sum_dir      REAL NOT NULL,
    -- "≥ 14 kn": forecast and measured, measured only, forecast only
    hits         INTEGER NOT NULL,
    misses       INTEGER NOT NULL,
    false_alarms INTEGER NOT NULL,
    PRIMARY KEY (station_id, model_id, day, bucket)
) WITHOUT ROWID;
