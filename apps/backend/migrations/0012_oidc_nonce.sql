-- Generic OpenID Connect: the nonce of an in-flight sign-in, checked against the ID token.
ALTER TABLE sso_states ADD COLUMN nonce text;
