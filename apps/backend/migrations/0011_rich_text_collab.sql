-- Real-time collaborative editing of rich-text fields. The row holds the merged Yjs document of one
-- (card, field) while it is being edited. The card's own data stays the source of truth: a write
-- through the API (other than the editors' own saves) discards the row, so the next session starts
-- from the card's current content.
CREATE TABLE collab_docs (
    card_id    uuid NOT NULL REFERENCES cards(id) ON DELETE CASCADE,
    slug       text NOT NULL,
    state      bytea NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (card_id, slug)
);
