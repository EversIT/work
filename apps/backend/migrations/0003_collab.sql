CREATE TABLE watchers (
    id            uuid PRIMARY KEY,
    user_id       uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    resource_type text NOT NULL CHECK (resource_type IN ('card','list')),
    resource_id   uuid NOT NULL,
    created_at    timestamptz NOT NULL DEFAULT now(),
    UNIQUE (user_id, resource_type, resource_id)
);
CREATE INDEX watchers_resource_idx ON watchers(resource_type, resource_id);

CREATE TABLE notifications (
    id                    uuid PRIMARY KEY,
    recipient_id          uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    workspace_id          uuid REFERENCES workspaces(id) ON DELETE CASCADE,
    type                  text NOT NULL CHECK (type IN ('system_announcement','subscription','mention','comment','assigned','unassigned','stage_updated','automation')),
    message               text NOT NULL CHECK (char_length(message) <= 1000),
    related_resource_type text CHECK (related_resource_type IN ('card','list')),
    related_resource_id   uuid,
    color                 text,
    is_read               boolean NOT NULL DEFAULT false,
    created_at            timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX notifications_recipient_idx ON notifications(recipient_id, is_read, created_at DESC);

CREATE TABLE notification_preferences (
    user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    channel text NOT NULL CHECK (channel IN ('in_app','slack','email')),
    enabled boolean NOT NULL DEFAULT true,
    config  jsonb NOT NULL DEFAULT '{}'::jsonb,
    PRIMARY KEY (user_id, channel)
);

CREATE TABLE files (
    id           uuid PRIMARY KEY,
    project_id   uuid NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    created_by   uuid REFERENCES users(id) ON DELETE SET NULL,
    name         text NOT NULL CHECK (char_length(name) <= 255),
    content_type text NOT NULL,
    size         bigint NOT NULL CHECK (size >= 0),
    storage_key  text NOT NULL UNIQUE,
    created_at   timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX files_project_idx ON files(project_id);
CREATE INDEX files_user_idx ON files(created_by);

ALTER TABLE projects ADD COLUMN user_upload_limit bigint NOT NULL DEFAULT 1073741824;
