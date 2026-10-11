-- Phase 2: water gauges of the federal waterways (PEGELONLINE, WSV) with their latest values,
-- refreshed every 15 minutes. The history is fetched from PEGELONLINE when a page shows it.
CREATE TABLE gauge (
    id             INTEGER PRIMARY KEY,
    uuid           TEXT NOT NULL UNIQUE,
    number         TEXT NOT NULL,
    name           TEXT NOT NULL,
    water          TEXT,
    -- river kilometre
    km             REAL,
    lat            REAL NOT NULL,
    lon            REAL NOT NULL,
    level_cm       REAL,
    level_at       INTEGER,
    -- PEGELONLINE's states: against mean low/high water (MNW/MHW) and against the lowest/highest
    -- navigable level (NSW/HSW): low, normal, high, unknown …
    level_state    TEXT,
    shipping_state TEXT,
    flow_m3s       REAL,
    flow_at        INTEGER,
    water_temp_c   REAL,
    water_temp_at  INTEGER,
    seen_at        INTEGER NOT NULL
);

CREATE INDEX gauge_lat_lon ON gauge (lat, lon);
