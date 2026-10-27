-- Single sign-on: external identities and the one-time state of an in-flight sign-in.

CREATE TABLE sso_identities (
    provider   text NOT NULL,          -- google | github | microsoft | slack | saml
    subject    text NOT NULL,          -- the provider's stable user id (never the e-mail address)
    user_id    uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    email      citext,                 -- what the provider reported when the identity was linked
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (provider, subject),
    UNIQUE (provider, user_id)
);

-- Random one-time `state` (only its hash is stored) bound to the browser that started the flow
-- through a cookie, so a callback cannot be injected into somebody else's browser.
CREATE TABLE sso_states (
    state_hash    bytea PRIMARY KEY,
    provider      text NOT NULL,
    binding_hash  bytea NOT NULL,
    code_verifier text,                -- PKCE (OAuth providers)
    saml_tracker  text,                -- SAML request tracker
    user_id       uuid REFERENCES users(id) ON DELETE CASCADE, -- set when an existing account links a provider
    next          text NOT NULL DEFAULT '/',
    expires_at    timestamptz NOT NULL
);
CREATE INDEX sso_states_expiry_idx ON sso_states(expires_at);
