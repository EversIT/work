-- Core work-management model: spaces, lists, stages, card types, fields, cards, activities, views.
-- Access model: project role (project_members) is the baseline. Spaces and lists can be
-- 'private'; then only project admins/owners and users with an explicit grant can see them.

CREATE TABLE spaces (
    id           uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    name         text NOT NULL CHECK (char_length(name) BETWEEN 1 AND 100),
    icon         text NOT NULL DEFAULT 'folder' CHECK (char_length(icon) <= 64),
    icon_color   text NOT NULL DEFAULT 'default' CHECK (char_length(icon_color) <= 32),
    access_type  text NOT NULL DEFAULT 'public' CHECK (access_type IN ('public','private')),
    created_by   uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at   timestamptz NOT NULL DEFAULT now(),
    deleted_at   timestamptz
);
CREATE INDEX spaces_workspace_idx ON spaces(workspace_id) WHERE deleted_at IS NULL;

CREATE TABLE card_types (
    id             uuid PRIMARY KEY,
    workspace_id   uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    name           text NOT NULL CHECK (char_length(name) BETWEEN 1 AND 100),
    layout         text NOT NULL DEFAULT 'default' CHECK (layout IN ('default','person','organization','deal')),
    title_template text CHECK (char_length(title_template) <= 255),
    has_children   boolean NOT NULL DEFAULT false,
    created_by     uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at     timestamptz NOT NULL DEFAULT now(),
    deleted_at     timestamptz
);
CREATE INDEX card_types_workspace_idx ON card_types(workspace_id) WHERE deleted_at IS NULL;

CREATE TABLE fields (
    id               uuid PRIMARY KEY,
    workspace_id     uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    card_type_id     uuid REFERENCES card_types(id) ON DELETE CASCADE,
    data_card_type_id uuid REFERENCES card_types(id) ON DELETE SET NULL,
    name             text NOT NULL CHECK (char_length(name) BETWEEN 1 AND 100),
    slug             text NOT NULL CHECK (slug ~ '^[a-z][a-z0-9_]{0,62}$'),
    type             text NOT NULL CHECK (type IN ('text','text_area','dropdown','label','user','date','date_time','number','percentage','checkbox','email','url','currency','stage','rich','card')),
    icon             text NOT NULL DEFAULT 'text' CHECK (char_length(icon) <= 64),
    required         boolean NOT NULL DEFAULT false,
    multiple         boolean NOT NULL DEFAULT false,
    items            jsonb,
    is_title         boolean NOT NULL DEFAULT false,
    is_description   boolean NOT NULL DEFAULT false,
    is_photo         boolean NOT NULL DEFAULT false,
    is_assignee      boolean NOT NULL DEFAULT false,
    is_pinned        boolean NOT NULL DEFAULT false,
    created_by       uuid REFERENCES users(id) ON DELETE SET NULL,
    created_by_type  text NOT NULL DEFAULT 'user' CHECK (created_by_type IN ('system','user')),
    created_at       timestamptz NOT NULL DEFAULT now(),
    deleted_at       timestamptz
);
-- A slug is unique per card type (NULL card type = workspace-wide field).
CREATE UNIQUE INDEX fields_slug_uniq ON fields (workspace_id, COALESCE(card_type_id, '00000000-0000-0000-0000-000000000000'::uuid), slug) WHERE deleted_at IS NULL;

CREATE UNIQUE INDEX fields_one_title ON fields (card_type_id) WHERE is_title AND deleted_at IS NULL AND card_type_id IS NOT NULL;

CREATE TABLE lists (
    id                   uuid PRIMARY KEY,
    workspace_id         uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    space_id             uuid NOT NULL REFERENCES spaces(id) ON DELETE CASCADE,
    default_card_type_id uuid REFERENCES card_types(id) ON DELETE SET NULL,
    name                 text NOT NULL CHECK (char_length(name) BETWEEN 1 AND 100),
    slug                 text NOT NULL CHECK (slug ~ '^[a-z0-9][a-z0-9-]{0,98}$'),
    icon                 text NOT NULL DEFAULT 'list' CHECK (char_length(icon) <= 64),
    icon_color           text NOT NULL DEFAULT 'default' CHECK (char_length(icon_color) <= 32),
    type                 text NOT NULL DEFAULT 'default_list' CHECK (type IN ('default_list','crm_contacts','crm_organizations','crm_deals')),
    access_type          text NOT NULL DEFAULT 'public' CHECK (access_type IN ('public','private')),
    created_by           uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at           timestamptz NOT NULL DEFAULT now(),
    deleted_at           timestamptz
);
CREATE INDEX lists_space_idx ON lists(space_id) WHERE deleted_at IS NULL;
CREATE UNIQUE INDEX lists_slug_uniq ON lists(workspace_id, slug) WHERE deleted_at IS NULL;

CREATE TABLE list_fields (
    list_id  uuid NOT NULL REFERENCES lists(id)  ON DELETE CASCADE,
    field_id uuid NOT NULL REFERENCES fields(id) ON DELETE CASCADE,
    PRIMARY KEY (list_id, field_id)
);

CREATE TABLE list_stages (
    id           uuid PRIMARY KEY,
    list_id      uuid NOT NULL REFERENCES lists(id) ON DELETE CASCADE,
    name         text NOT NULL CHECK (char_length(name) BETWEEN 1 AND 100),
    color        text NOT NULL DEFAULT 'default' CHECK (char_length(color) <= 32),
    sort_order   integer NOT NULL DEFAULT 0,
    is_completed boolean NOT NULL DEFAULT false
);
CREATE INDEX list_stages_list_idx ON list_stages(list_id, sort_order);

CREATE TABLE list_groups (
    id          uuid PRIMARY KEY,
    list_id     uuid NOT NULL REFERENCES lists(id) ON DELETE CASCADE,
    field_id    uuid REFERENCES fields(id) ON DELETE CASCADE,
    name        text NOT NULL CHECK (char_length(name) BETWEEN 1 AND 100),
    type        text NOT NULL CHECK (type IN ('all','list_stage','field')),
    color       text CHECK (char_length(color) <= 32),
    icon        text CHECK (char_length(icon) <= 64),
    sort_order  integer NOT NULL DEFAULT 0,
    is_expanded boolean NOT NULL DEFAULT true,
    filter      jsonb
);
CREATE INDEX list_groups_list_idx ON list_groups(list_id, sort_order);

CREATE TABLE views (
    id         uuid PRIMARY KEY,
    list_id    uuid NOT NULL REFERENCES lists(id) ON DELETE CASCADE,
    name       text NOT NULL CHECK (char_length(name) BETWEEN 1 AND 100),
    type       text NOT NULL DEFAULT 'table' CHECK (type IN ('table','board','calendar','gantt','list')),
    options    jsonb NOT NULL DEFAULT '{}'::jsonb,
    filters    jsonb,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX views_list_idx ON views(list_id);

CREATE TABLE cards (
    id              uuid PRIMARY KEY,
    workspace_id    uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    card_type_id    uuid NOT NULL REFERENCES card_types(id),
    parent_id       uuid REFERENCES cards(id) ON DELETE CASCADE,
    data            jsonb NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(data) = 'object'),
    created_by      uuid REFERENCES users(id) ON DELETE SET NULL,
    created_by_type text NOT NULL DEFAULT 'user' CHECK (created_by_type IN ('system','user')),
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    deleted_at      timestamptz
);
CREATE INDEX cards_workspace_idx ON cards(workspace_id) WHERE deleted_at IS NULL;
CREATE INDEX cards_parent_idx ON cards(parent_id);
CREATE INDEX cards_data_gin ON cards USING gin (data jsonb_path_ops);

CREATE TABLE card_lists (
    id            uuid PRIMARY KEY,
    card_id       uuid NOT NULL REFERENCES cards(id) ON DELETE CASCADE,
    list_id       uuid NOT NULL REFERENCES lists(id) ON DELETE CASCADE,
    list_stage_id uuid NOT NULL REFERENCES list_stages(id),
    sort_order    integer NOT NULL DEFAULT 100,
    created_at    timestamptz NOT NULL DEFAULT now(),
    UNIQUE (card_id, list_id)
);
CREATE INDEX card_lists_list_idx ON card_lists(list_id, list_stage_id, sort_order);

CREATE TABLE card_activities (
    id              uuid PRIMARY KEY,
    card_id         uuid NOT NULL REFERENCES cards(id) ON DELETE CASCADE,
    type            text NOT NULL CHECK (type IN ('update','comment','email','task','call','message','meeting')),
    content         jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_by      uuid REFERENCES users(id) ON DELETE SET NULL,
    created_by_type text NOT NULL DEFAULT 'user' CHECK (created_by_type IN ('system','user')),
    created_at      timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX card_activities_card_idx ON card_activities(card_id, created_at);

-- Explicit grants on private spaces/lists.
CREATE TABLE resource_grants (
    resource_type text NOT NULL CHECK (resource_type IN ('space','list')),
    resource_id   uuid NOT NULL,
    user_id       uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role          text NOT NULL CHECK (role IN ('viewer','member','admin')),
    PRIMARY KEY (resource_type, resource_id, user_id)
);
CREATE INDEX resource_grants_user_idx ON resource_grants(user_id);
