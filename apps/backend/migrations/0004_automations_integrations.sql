CREATE TABLE automations (
    id             uuid PRIMARY KEY,
    workspace_id   uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    name           text NOT NULL CHECK (char_length(name) BETWEEN 1 AND 100),
    is_enabled     boolean NOT NULL DEFAULT false,
    trigger_type   text NOT NULL CHECK (trigger_type IN ('card_created','field_updated','stage_changed','comment_created','assignee_added','assignee_removed')),
    trigger_config jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_by     uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now(),
    deleted_at     timestamptz
);
CREATE INDEX automations_workspace_idx ON automations(workspace_id) WHERE deleted_at IS NULL;

-- Where an automation applies. No rows = every list of the workspace.
CREATE TABLE automation_locations (
    automation_id uuid NOT NULL REFERENCES automations(id) ON DELETE CASCADE,
    location_type text NOT NULL CHECK (location_type IN ('space','list')),
    location_id   uuid NOT NULL,
    PRIMARY KEY (automation_id, location_type, location_id)
);

CREATE TABLE automation_steps (
    id            uuid PRIMARY KEY,
    automation_id uuid NOT NULL REFERENCES automations(id) ON DELETE CASCADE,
    sort_order    integer NOT NULL,
    action_type   text NOT NULL CHECK (action_type IN ('create_card','create_sub_card','set_field','create_comment','change_assignee','set_stage')),
    config        jsonb NOT NULL DEFAULT '{}'::jsonb
);
CREATE INDEX automation_steps_idx ON automation_steps(automation_id, sort_order);

CREATE TABLE automation_runs (
    id            uuid PRIMARY KEY,
    automation_id uuid NOT NULL REFERENCES automations(id) ON DELETE CASCADE,
    card_id       uuid REFERENCES cards(id) ON DELETE SET NULL,
    status        text NOT NULL CHECK (status IN ('running','completed','failed')),
    error         text,
    started_at    timestamptz NOT NULL DEFAULT now(),
    finished_at   timestamptz
);
CREATE INDEX automation_runs_idx ON automation_runs(automation_id, started_at DESC);

-- Third-party connections. Tokens are stored encrypted (AES-256-GCM), never as plaintext JSON.
CREATE TABLE user_integrations (
    id            uuid PRIMARY KEY,
    user_id       uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    type          text NOT NULL CHECK (type IN ('slack')),
    secret        bytea NOT NULL,
    public_config jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at    timestamptz NOT NULL DEFAULT now(),
    UNIQUE (user_id, type)
);

-- One-time OAuth `state` values, bound to the user who started the flow.
CREATE TABLE oauth_states (
    state_hash  bytea PRIMARY KEY,
    user_id     uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    integration text NOT NULL,
    expires_at  timestamptz NOT NULL
);
