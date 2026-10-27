-- WebAuthn passkeys: public-key credentials that sign users in without a password.
CREATE TABLE passkeys (
    id            uuid PRIMARY KEY,
    user_id       uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    credential_id bytea NOT NULL UNIQUE,
    public_key    bytea NOT NULL,          -- SEC1 uncompressed P-256 point
    sign_count    bigint NOT NULL DEFAULT 0,
    name          text NOT NULL CHECK (char_length(name) BETWEEN 1 AND 100),
    created_at    timestamptz NOT NULL DEFAULT now(),
    last_used_at  timestamptz
);
CREATE INDEX passkeys_user_idx ON passkeys(user_id);

-- One-time challenges of ceremonies in progress.
CREATE TABLE webauthn_challenges (
    id         uuid PRIMARY KEY,
    kind       text NOT NULL CHECK (kind IN ('register','login')),
    user_id    uuid REFERENCES users(id) ON DELETE CASCADE,
    challenge  bytea NOT NULL,
    expires_at timestamptz NOT NULL
);
CREATE INDEX webauthn_challenges_expiry_idx ON webauthn_challenges(expires_at);
