-- agents.db v4: who signed an installed package, and the keys the user trusts.
--
-- trusted_keys : the public keys of the publishers the user trusts, by the
--                SHA-256 fingerprint of the key. `origin` says who put the key
--                there: the user, by importing it, or the deposited policy.
--
-- installed_packages gains three columns, added by the Rust step of the
-- migration (crate::agents_db::migrate_v4) after a probe, so a second run
-- leaves them as they are: `publisher` (the name the package declares),
-- `key_fingerprint` (the fingerprint of the key that signed it, NULL when it
-- is not signed) and `signed`. Whether the publisher is trusted is not
-- stored: it is read from trusted_keys whenever it is shown.

CREATE TABLE IF NOT EXISTS trusted_keys (
    fingerprint TEXT PRIMARY KEY,
    public_key  TEXT NOT NULL,
    label       TEXT NOT NULL,
    origin      TEXT NOT NULL DEFAULT 'user' CHECK (origin IN ('user', 'policy')),
    added_at    TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);
