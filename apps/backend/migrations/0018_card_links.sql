-- Links between issues ("blocks", "duplicates", "relates to", "clones"), like Jira's linked work
-- items. A row reads "source <type> target"; the other direction ("is blocked by") is the same row
-- seen from the target. Both cards live in one workspace, which the API checks.
CREATE TABLE card_links (
    id             uuid PRIMARY KEY,
    source_card_id uuid NOT NULL REFERENCES cards(id) ON DELETE CASCADE,
    target_card_id uuid NOT NULL REFERENCES cards(id) ON DELETE CASCADE,
    link_type      text NOT NULL CHECK (link_type IN ('blocks','duplicates','relates','clones')),
    created_by     uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at     timestamptz NOT NULL DEFAULT now(),
    CHECK (source_card_id <> target_card_id),
    UNIQUE (source_card_id, target_card_id, link_type)
);
CREATE INDEX card_links_target_idx ON card_links(target_card_id);
