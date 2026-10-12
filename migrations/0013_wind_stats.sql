-- A station's wind over whole years (Phase 4, wind statistics): the summary only, as JSON
-- (wf_core::climate::WindStats), made from an archive's hourly values once a year.
CREATE TABLE wind_stats (
    station_id  INTEGER PRIMARY KEY REFERENCES station (id) ON DELETE CASCADE,
    -- the last whole year in it; made anew once a newer year has ended
    last_year   INTEGER NOT NULL,
    computed_at INTEGER NOT NULL,
    -- NULL when the archive had no hours for the station
    stats       TEXT
) WITHOUT ROWID;
