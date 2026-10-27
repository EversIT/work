-- Account lockout, multi-factor authentication, e-mail verification at sign-up and a shared
-- rate limiter.

ALTER TABLE users
    ADD COLUMN failed_logins     integer     NOT NULL DEFAULT 0,
    ADD COLUMN last_failed_at    timestamptz,
    ADD COLUMN locked_until      timestamptz,
    -- AES-256-GCM(secret), bound to the user id. `mfa_enabled_at` is set once a code was confirmed.
    ADD COLUMN mfa_secret        bytea,
    ADD COLUMN mfa_enabled_at    timestamptz,
    -- Last accepted TOTP time step: a code can be used once (replay protection).
    ADD COLUMN mfa_last_step     bigint;

-- Single-use recovery codes. Only a SHA-256 is stored.
CREATE TABLE mfa_recovery_codes (
    user_id   uuid  NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    code_hash bytea NOT NULL,
    used_at   timestamptz,
    PRIMARY KEY (user_id, code_hash)
);

-- Second step of a sign-in: issued after the password was correct, redeemed with a code.
CREATE TABLE mfa_challenges (
    token_hash bytea PRIMARY KEY,
    user_id    uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    attempts   integer NOT NULL DEFAULT 0,
    expires_at timestamptz NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX mfa_challenges_user_idx ON mfa_challenges(user_id);

-- A registration is only turned into an account once the address was confirmed, so an attacker
-- cannot pre-register somebody else's address and cannot learn whether an address is taken.
CREATE TABLE pending_registrations (
    email         citext PRIMARY KEY,
    password_hash text NOT NULL,
    first_name    text NOT NULL,
    last_name     text NOT NULL,
    token_hash    bytea NOT NULL UNIQUE,
    expires_at    timestamptz NOT NULL,
    created_at    timestamptz NOT NULL DEFAULT now()
);

-- Fixed-window counters for the limiter that is shared between instances.
CREATE TABLE rate_limits (
    key          text PRIMARY KEY,
    window_start timestamptz NOT NULL,
    hits         integer NOT NULL
);
CREATE INDEX rate_limits_window_idx ON rate_limits(window_start);
