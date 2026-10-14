-- Counters per UTC day that must outlive a restart: Open-Meteo calls (the free API allows about
-- 10,000 a day, and a counter kept only in memory started again at 0 after every restart) and
-- the e-mails the server sends. `day` is the number of days since 1970-01-01 (UTC).
CREATE TABLE usage_daily (
    day     INTEGER NOT NULL,
    counter TEXT NOT NULL,
    n       INTEGER NOT NULL,
    PRIMARY KEY (day, counter)
) WITHOUT ROWID;
