-- Instance administration: branding, blocked accounts and an audit log of what admins change.

ALTER TABLE users ADD COLUMN disabled_at timestamptz;

-- At most one row: the instance's name, description, accent colour and favicon.
CREATE TABLE branding (
    id           boolean PRIMARY KEY DEFAULT true CHECK (id),
    title        text NOT NULL DEFAULT 'work' CHECK (char_length(title) BETWEEN 1 AND 60),
    description  text NOT NULL DEFAULT 'Open-source work management' CHECK (char_length(description) <= 300),
    accent       text NOT NULL DEFAULT '#0C66E4' CHECK (accent ~ '^#[0-9A-Fa-f]{6}$'),
    favicon      bytea,
    favicon_type text,
    updated_at   timestamptz NOT NULL DEFAULT now(),
    updated_by   uuid REFERENCES users(id) ON DELETE SET NULL
);
INSERT INTO branding (id) VALUES (true);

CREATE TABLE admin_audit (
    id         bigserial PRIMARY KEY,
    at         timestamptz NOT NULL DEFAULT now(),
    actor      uuid REFERENCES users(id) ON DELETE SET NULL,
    action     text NOT NULL,
    target     text,
    detail     jsonb NOT NULL DEFAULT '{}'::jsonb
);
CREATE INDEX admin_audit_at_idx ON admin_audit(at DESC);
