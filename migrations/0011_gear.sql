-- Phase 4: a user's gear (kites, wings, windsurf sails, parawings), each with the mean wind it
-- works in. The kind only names it; every kind is reckoned alike.
CREATE TABLE gear (
    id         INTEGER PRIMARY KEY,
    account_id INTEGER NOT NULL REFERENCES account (id) ON DELETE CASCADE,
    kind       TEXT NOT NULL CHECK (kind IN ('kite', 'wing', 'windsurf', 'parawing')),
    -- m²
    size       REAL NOT NULL CHECK (size > 0),
    -- kn
    min_kn     REAL NOT NULL CHECK (min_kn >= 0),
    max_kn     REAL NOT NULL,
    position   INTEGER NOT NULL,
    CHECK (max_kn > min_kn)
);
CREATE INDEX gear_account ON gear (account_id, position);
