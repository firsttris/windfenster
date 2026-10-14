-- The admin page. An account is a user or an admin; an admin is made on the server
-- (`windfenster admin`), never over the web. A disabled account cannot sign in, its sessions are
-- gone and its alarms rest; its data stays until it is deleted. `last_seen_at` is when a session
-- of it was last used, at most a day off (sessions are renewed daily).
ALTER TABLE account ADD COLUMN role TEXT NOT NULL DEFAULT 'user' CHECK (role IN ('user', 'admin'));
ALTER TABLE account ADD COLUMN disabled_at INTEGER;
ALTER TABLE account ADD COLUMN last_seen_at INTEGER;

-- Settings changed on the admin page, as JSON values. An environment variable that is set wins
-- over a row here; without either the default in the code holds.
CREATE TABLE setting (
    key        TEXT PRIMARY KEY,
    value      TEXT NOT NULL,
    updated_at INTEGER NOT NULL
) WITHOUT ROWID;

-- What admins changed: who, when, what (a code), and on what (an account, a setting key ...).
CREATE TABLE admin_log (
    id         INTEGER PRIMARY KEY,
    at         INTEGER NOT NULL,
    account_id INTEGER REFERENCES account (id) ON DELETE SET NULL,
    action     TEXT NOT NULL,
    target     TEXT
);
CREATE INDEX admin_log_at ON admin_log (at);

-- Invitations for when sign-up needs one: a link by the SHA-256 of its token, used once.
CREATE TABLE invite (
    token_hash BLOB PRIMARY KEY,
    created_by INTEGER REFERENCES account (id) ON DELETE SET NULL,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    used_at    INTEGER,
    used_by    INTEGER REFERENCES account (id) ON DELETE SET NULL
);

-- Every alarm message handed to a channel, delivered or not, kept 30 days.
CREATE TABLE alarm_delivery (
    id          INTEGER PRIMARY KEY,
    sent_at     INTEGER NOT NULL,
    account_id  INTEGER REFERENCES account (id) ON DELETE CASCADE,
    favorite_id INTEGER,
    -- 'push' or 'email'
    channel     TEXT NOT NULL,
    -- 'window', 'live' or 'test'
    kind        TEXT NOT NULL,
    title       TEXT NOT NULL,
    ok          INTEGER NOT NULL,
    -- why it failed, e.g. 'push 410' or 'e-mail: limit'
    detail      TEXT
);
CREATE INDEX alarm_delivery_sent ON alarm_delivery (sent_at);
