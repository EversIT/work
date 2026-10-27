CREATE EXTENSION IF NOT EXISTS citext;

CREATE TABLE users (
    id            uuid PRIMARY KEY,
    email         citext NOT NULL UNIQUE,
    password_hash text   NOT NULL,
    first_name    text   NOT NULL CHECK (char_length(first_name) BETWEEN 1 AND 100),
    last_name     text   NOT NULL CHECK (char_length(last_name)  BETWEEN 1 AND 100),
    is_instance_admin boolean NOT NULL DEFAULT false,
    created_at    timestamptz NOT NULL DEFAULT now(),
    deleted_at    timestamptz
);

-- Refresh-token sessions. Only a SHA-256 of the token is stored.
CREATE TABLE sessions (
    id         uuid PRIMARY KEY,
    family_id  uuid NOT NULL,
    user_id    uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash bytea NOT NULL UNIQUE,
    expires_at timestamptz NOT NULL,
    revoked_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX sessions_user_idx ON sessions(user_id);
CREATE INDEX sessions_family_idx ON sessions(family_id);

CREATE TABLE projects (
    id         uuid PRIMARY KEY,
    name       text NOT NULL CHECK (char_length(name) BETWEEN 1 AND 100),
    created_by uuid NOT NULL REFERENCES users(id),
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE project_members (
    project_id uuid NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    user_id    uuid NOT NULL REFERENCES users(id)    ON DELETE CASCADE,
    role       text NOT NULL CHECK (role IN ('viewer','member','admin','owner')),
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (project_id, user_id)
);
CREATE INDEX project_members_user_idx ON project_members(user_id);

-- Invites: random secret, only its hash is stored; expiring and use-limited.
CREATE TABLE invites (
    id         uuid PRIMARY KEY,
    project_id uuid NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    token_hash bytea NOT NULL UNIQUE,
    role       text NOT NULL CHECK (role IN ('viewer','member','admin')),
    expires_at timestamptz NOT NULL,
    max_uses   integer NOT NULL CHECK (max_uses BETWEEN 1 AND 100),
    uses       integer NOT NULL DEFAULT 0,
    created_by uuid NOT NULL REFERENCES users(id),
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE workspaces (
    id         uuid PRIMARY KEY,
    project_id uuid NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name       text NOT NULL CHECK (char_length(name) BETWEEN 1 AND 100),
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX workspaces_project_idx ON workspaces(project_id);
