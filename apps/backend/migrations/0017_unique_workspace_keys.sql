-- Issue keys identify a workspace instance-wide (/projects/ACME, /browse/ACME-12).
-- Existing duplicates: the oldest workspace keeps its prefix, later ones get the prefix (shortened
-- to stay within 10 characters) plus a number: WRK, WRK2, WRK3, ... never an existing value.
DO $$
DECLARE
    r record;
    n integer;
    candidate text;
BEGIN
    FOR r IN
        SELECT id, key_prefix FROM (
            SELECT id, key_prefix,
                   row_number() OVER (PARTITION BY key_prefix ORDER BY created_at, id) AS rn
            FROM workspaces
        ) d
        WHERE rn > 1
        ORDER BY key_prefix, rn
    LOOP
        n := 2;
        LOOP
            candidate := left(r.key_prefix, 10 - length(n::text)) || n::text;
            EXIT WHEN NOT EXISTS (SELECT 1 FROM workspaces WHERE key_prefix = candidate);
            n := n + 1;
        END LOOP;
        UPDATE workspaces SET key_prefix = candidate WHERE id = r.id;
    END LOOP;
END $$;

CREATE UNIQUE INDEX workspaces_key_prefix_uq ON workspaces (key_prefix);
