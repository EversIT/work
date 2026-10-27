-- Limited-access ("restricted") members: people added to a single list. They are viewers of the
-- project but behave as if every space and list were private, so they only see what they hold an
-- explicit resource grant on.
ALTER TABLE project_members
    ADD COLUMN restricted boolean NOT NULL DEFAULT false,
    ADD CONSTRAINT project_members_restricted_viewer CHECK (NOT restricted OR role = 'viewer');

-- An invite with a list_id adds the invitee to that list only (restricted viewer + list grant).
ALTER TABLE invites ADD COLUMN list_id uuid REFERENCES lists(id) ON DELETE CASCADE;
