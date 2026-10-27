//! Source id -> new id of everything the import created, kept in `migration_ids` so a second run
//! skips what exists.

use std::collections::HashMap;

use sqlx::PgPool;
use uuid::Uuid;

pub(super) struct IdMap {
    db: PgPool,
    cache: HashMap<(&'static str, String), Uuid>,
}

impl IdMap {
    pub(super) fn new(db: PgPool) -> Self {
        Self {
            db,
            cache: HashMap::new(),
        }
    }

    pub(super) async fn get(
        &mut self,
        kind: &'static str,
        source: &str,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        if let Some(id) = self.cache.get(&(kind, source.to_owned())) {
            return Ok(Some(*id));
        }
        let row: Option<(Uuid,)> =
            sqlx::query_as("SELECT id FROM migration_ids WHERE kind = $1 AND source_id = $2")
                .bind(kind)
                .bind(source)
                .fetch_optional(&self.db)
                .await?;
        if let Some((id,)) = row {
            self.cache.insert((kind, source.to_owned()), id);
        }
        Ok(row.map(|r| r.0))
    }

    pub(super) async fn put(
        &mut self,
        kind: &'static str,
        source: &str,
        id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("INSERT INTO migration_ids (kind, source_id, id) VALUES ($1,$2,$3) ON CONFLICT DO NOTHING")
            .bind(kind)
            .bind(source)
            .bind(id)
            .execute(&self.db)
            .await?;
        self.cache.insert((kind, source.to_owned()), id);
        Ok(())
    }
}
