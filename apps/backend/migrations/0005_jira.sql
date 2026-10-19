-- Jira-style model: issue keys, issue types, sprints, API tokens, password resets, webhooks.

-- Issue keys: WORK-12 = workspace key prefix + per-workspace sequence.
ALTER TABLE workspaces ADD COLUMN key_prefix text, ADD COLUMN next_issue_number integer NOT NULL DEFAULT 1;
UPDATE workspaces SET key_prefix = upper(left(regexp_replace(name, '[^A-Za-z0-9]', '', 'g'), 4));
UPDATE workspaces SET key_prefix = 'WRK' WHERE key_prefix IS NULL OR key_prefix !~ '^[A-Z][A-Z0-9]{1,9}$';
ALTER TABLE workspaces ALTER COLUMN key_prefix SET NOT NULL;
ALTER TABLE workspaces ADD CONSTRAINT workspaces_key_prefix_chk CHECK (key_prefix ~ '^[A-Z][A-Z0-9]{1,9}$');

ALTER TABLE cards ADD COLUMN number integer;
UPDATE cards c SET number = n.rn FROM (
    SELECT id, row_number() OVER (PARTITION BY workspace_id ORDER BY created_at, id) AS rn FROM cards
) n WHERE n.id = c.id;
UPDATE workspaces w SET next_issue_number = COALESCE((SELECT max(number) + 1 FROM cards WHERE workspace_id = w.id), 1);
ALTER TABLE cards ALTER COLUMN number SET NOT NULL;
CREATE UNIQUE INDEX cards_number_uniq ON cards (workspace_id, number);

-- Issue types are card types with an icon and a colour.
ALTER TABLE card_types ADD COLUMN icon text NOT NULL DEFAULT 'task' CHECK (char_length(icon) <= 32),
                       ADD COLUMN color text NOT NULL DEFAULT 'blue' CHECK (char_length(color) <= 32);

-- Workspace-wide fields: at most one title field.
CREATE UNIQUE INDEX fields_one_title_global ON fields (workspace_id) WHERE is_title AND deleted_at IS NULL AND card_type_id IS NULL;

-- Kanban (board only) or Scrum (backlog + sprints).
ALTER TABLE lists ADD COLUMN methodology text NOT NULL DEFAULT 'kanban' CHECK (methodology IN ('kanban','scrum'));

CREATE TABLE sprints (
    id           uuid PRIMARY KEY,
    list_id      uuid NOT NULL REFERENCES lists(id) ON DELETE CASCADE,
    name         text NOT NULL CHECK (char_length(name) BETWEEN 1 AND 100),
    goal         text CHECK (char_length(goal) <= 1000),
    state        text NOT NULL DEFAULT 'planned' CHECK (state IN ('planned','active','completed')),
    starts_at    timestamptz,
    ends_at      timestamptz,
    completed_at timestamptz,
    created_at   timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX sprints_list_idx ON sprints(list_id, created_at);
-- One active sprint per list.
CREATE UNIQUE INDEX sprints_one_active ON sprints(list_id) WHERE state = 'active';

ALTER TABLE card_lists ADD COLUMN sprint_id uuid REFERENCES sprints(id) ON DELETE SET NULL;
CREATE INDEX card_lists_sprint_idx ON card_lists(sprint_id);

-- Personal API tokens (Jira-style). Only a SHA-256 of the token is stored.
CREATE TABLE api_tokens (
    id           uuid PRIMARY KEY,
    user_id      uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name         text NOT NULL CHECK (char_length(name) BETWEEN 1 AND 100),
    token_hash   bytea NOT NULL UNIQUE,
    token_prefix text NOT NULL,
    scope        text NOT NULL CHECK (scope IN ('read','write')),
    expires_at   timestamptz,
    last_used_at timestamptz,
    revoked_at   timestamptz,
    created_at   timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX api_tokens_user_idx ON api_tokens(user_id);

CREATE TABLE password_resets (
    token_hash bytea PRIMARY KEY,
    user_id    uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    expires_at timestamptz NOT NULL,
    used_at    timestamptz
);
CREATE INDEX password_resets_user_idx ON password_resets(user_id);

-- Webhooks for automations: inbound trigger and outbound/email actions.
ALTER TABLE automations ADD COLUMN webhook_token_hash bytea UNIQUE;
ALTER TABLE automations DROP CONSTRAINT automations_trigger_type_check;
ALTER TABLE automations ADD CONSTRAINT automations_trigger_type_check
    CHECK (trigger_type IN ('card_created','field_updated','stage_changed','comment_created','assignee_added','assignee_removed','webhook'));
ALTER TABLE automation_steps DROP CONSTRAINT automation_steps_action_type_check;
ALTER TABLE automation_steps ADD CONSTRAINT automation_steps_action_type_check
    CHECK (action_type IN ('create_card','create_sub_card','set_field','create_comment','change_assignee','set_stage','send_webhook','send_email'));
ALTER TABLE automation_runs ALTER COLUMN card_id DROP NOT NULL;
