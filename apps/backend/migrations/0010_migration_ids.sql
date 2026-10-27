-- Source id -> new id of everything the one-off upstream import created, so a second run skips
-- what exists. Empty on instances that never ran the import.
CREATE TABLE migration_ids (
    kind      text NOT NULL,
    source_id text NOT NULL,
    id        uuid NOT NULL,
    PRIMARY KEY (kind, source_id)
);
