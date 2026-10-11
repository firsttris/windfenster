-- Phase 4: accounts. Sign-in with passkeys only (WebAuthn); a user has a name and one or more
-- passkeys, sessions are random tokens of which only the SHA-256 is kept.
CREATE TABLE account (
    id         INTEGER PRIMARY KEY,
    -- WebAuthn user handle, a UUID: what a passkey names its user by
    handle     TEXT NOT NULL UNIQUE,
    name       TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE TABLE passkey (
    -- credential id, hex
    id           TEXT PRIMARY KEY,
    account_id   INTEGER NOT NULL REFERENCES account (id) ON DELETE CASCADE,
    -- the credential as webauthn-rs serialises it (public key, counter, flags)
    credential   TEXT NOT NULL,
    created_at   INTEGER NOT NULL,
    last_used_at INTEGER
);
CREATE INDEX passkey_account ON passkey (account_id);

CREATE TABLE session (
    token_hash BLOB PRIMARY KEY,
    account_id INTEGER NOT NULL REFERENCES account (id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL
) WITHOUT ROWID;
CREATE INDEX session_account ON session (account_id);

-- A favourite: a station (by slug, which outlives a re-import) or a point on the 0.01° grid,
-- with the user's own name and, optionally, the wind directions that work there (a sector
-- from `wind_from` clockwise to `wind_to`, degrees the wind comes from).
CREATE TABLE favorite (
    id         INTEGER PRIMARY KEY,
    account_id INTEGER NOT NULL REFERENCES account (id) ON DELETE CASCADE,
    slug       TEXT,
    lat        REAL NOT NULL,
    lon        REAL NOT NULL,
    name       TEXT NOT NULL,
    wind_from  INTEGER CHECK (wind_from BETWEEN 0 AND 359),
    wind_to    INTEGER CHECK (wind_to BETWEEN 0 AND 359),
    position   INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    CHECK ((wind_from IS NULL) = (wind_to IS NULL))
);
CREATE INDEX favorite_account ON favorite (account_id, position);
