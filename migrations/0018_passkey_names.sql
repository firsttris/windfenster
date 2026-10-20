-- A passkey's name on the account page: the browser and system it was added with (read from the
-- User-Agent then; NULL when unknown or added before) and the user's own name for it (NULL =
-- the browser and system).
ALTER TABLE passkey ADD COLUMN browser TEXT;
ALTER TABLE passkey ADD COLUMN os TEXT;
ALTER TABLE passkey ADD COLUMN name TEXT;
