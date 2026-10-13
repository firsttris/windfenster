-- Alarm e-mails go only to an address its owner confirmed by a link: otherwise anyone could sign
-- up with someone else's address and have the server mail them. Addresses already stored were
-- entered by the people running and using this instance, so they count as confirmed.
ALTER TABLE account ADD COLUMN email_confirmed INTEGER NOT NULL DEFAULT 0;
UPDATE account SET email_confirmed = 1 WHERE email IS NOT NULL;

-- A link sent to confirm an address, by the SHA-256 of its token; it confirms only the address it
-- was sent to, and only once.
CREATE TABLE email_confirmation (
    token_hash BLOB PRIMARY KEY,
    account_id INTEGER NOT NULL REFERENCES account (id) ON DELETE CASCADE,
    email      TEXT NOT NULL,
    expires_at INTEGER NOT NULL
);
CREATE INDEX email_confirmation_account ON email_confirmation (account_id);
