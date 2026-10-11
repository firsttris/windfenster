-- Phase 4: signing in with e-mail and password too, next to passkeys. Both are optional on an
-- account; one with neither cannot sign in. The hash is Argon2id in PHC form.
ALTER TABLE account ADD COLUMN email TEXT;
ALTER TABLE account ADD COLUMN password_hash TEXT;
CREATE UNIQUE INDEX account_email ON account (email) WHERE email IS NOT NULL;
