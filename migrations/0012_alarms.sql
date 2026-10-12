-- Phase 4: alarms per favourite, by web push and e-mail.

-- How the account's messages are written: language, time zone (IANA) and wind unit, as its
-- browser had them when an alarm was last set.
ALTER TABLE account ADD COLUMN locale TEXT NOT NULL DEFAULT 'de';
ALTER TABLE account ADD COLUMN time_zone TEXT NOT NULL DEFAULT 'Europe/Berlin';
ALTER TABLE account ADD COLUMN unit TEXT NOT NULL DEFAULT 'kn';

-- One alarm per favourite.
CREATE TABLE alarm (
    favorite_id INTEGER PRIMARY KEY REFERENCES favorite (id) ON DELETE CASCADE,
    -- the least mean wind that counts, kn
    min_kn      REAL NOT NULL CHECK (min_kn >= 0),
    -- 1: only hours some piece of the account's gear fits
    gear        INTEGER NOT NULL DEFAULT 0,
    -- 1: tell ahead from the forecast (and when that window changes)
    forecast    INTEGER NOT NULL DEFAULT 1,
    -- 1: tell when the station measures it (stations only)
    live        INTEGER NOT NULL DEFAULT 0,
    -- channels
    push        INTEGER NOT NULL DEFAULT 1,
    email       INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL
);

-- Browsers that take the account's push messages (the Push API's subscription).
CREATE TABLE push_subscription (
    endpoint   TEXT PRIMARY KEY,
    account_id INTEGER NOT NULL REFERENCES account (id) ON DELETE CASCADE,
    p256dh     TEXT NOT NULL,
    auth       TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE INDEX push_subscription_account ON push_subscription (account_id);

-- What an alarm told, per local day (yyyymmdd in the account's time zone): the forecast window
-- announced, so that only a real change is told again, and whether the live alarm went out.
CREATE TABLE alarm_sent (
    favorite_id INTEGER NOT NULL REFERENCES alarm (favorite_id) ON DELETE CASCADE,
    day         INTEGER NOT NULL,
    kind        TEXT NOT NULL CHECK (kind IN ('window', 'live')),
    -- the window: start and end (Unix seconds) and its strongest wind (kn); NULL once cancelled
    start_at    INTEGER,
    end_at      INTEGER,
    peak_kn     REAL,
    sent_at     INTEGER NOT NULL,
    PRIMARY KEY (favorite_id, day, kind)
) WITHOUT ROWID;
