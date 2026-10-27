//! The export contract (the views an operator creates in the source database) and its check.

use std::collections::{HashMap, HashSet};

use sqlx::PgPool;

/// The export contract: view -> required columns.
pub const CONTRACT: &[(&str, &[&str])] = &[
    (
        "users",
        &[
            "id",
            "email",
            "first_name",
            "last_name",
            "password_hash",
            "created_at",
        ],
    ),
    ("projects", &["id", "name"]),
    ("project_members", &["project_id", "user_id", "role"]),
    ("workspaces", &["id", "project_id", "name", "key_prefix"]),
    ("spaces", &["id", "workspace_id", "name", "is_private"]),
    ("lists", &["id", "space_id", "name", "methodology"]),
    (
        "stages",
        &["id", "list_id", "name", "sort_order", "is_completed"],
    ),
    ("card_types", &["id", "workspace_id", "name", "layout"]),
    (
        "fields",
        &[
            "id",
            "workspace_id",
            "card_type_id",
            "name",
            "slug",
            "type",
            "items",
            "multiple",
            "required",
        ],
    ),
    (
        "cards",
        &[
            "id",
            "workspace_id",
            "card_type_id",
            "parent_id",
            "data",
            "created_by",
            "created_at",
        ],
    ),
    ("card_lists", &["card_id", "list_id", "stage_id"]),
    (
        "comments",
        &["id", "card_id", "user_id", "content", "created_at"],
    ),
];

#[derive(Debug, thiserror::Error)]
pub enum MigrateError {
    #[error("{0}")]
    Contract(String),
    #[error("database: {0}")]
    Db(#[from] sqlx::Error),
}

fn valid_schema_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 63
        && s.starts_with(|c: char| c.is_ascii_lowercase() || c == '_')
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Fails with a precise message when a view or column of the contract is missing.
pub async fn check_contract(source: &PgPool, schema: &str) -> Result<(), MigrateError> {
    if !valid_schema_name(schema) {
        return Err(MigrateError::Contract(
            "the schema name must be lower-case letters, digits and underscores".into(),
        ));
    }
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT table_name::text, column_name::text FROM information_schema.columns WHERE table_schema = $1",
    )
    .bind(schema)
    .fetch_all(source)
    .await?;
    let mut have: HashMap<&str, HashSet<&str>> = HashMap::new();
    for (t, c) in &rows {
        have.entry(t.as_str()).or_default().insert(c.as_str());
    }
    let mut missing = Vec::new();
    for (view, cols) in CONTRACT {
        match have.get(view) {
            None => missing.push(format!("{schema}.{view} does not exist")),
            Some(present) => {
                for c in *cols {
                    if !present.contains(c) {
                        missing.push(format!("{schema}.{view} lacks column {c}"));
                    }
                }
            }
        }
    }
    if missing.is_empty() {
        Ok(())
    } else {
        Err(MigrateError::Contract(format!(
            "the export views do not match the contract:\n  - {}",
            missing.join("\n  - ")
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_names_are_restricted() {
        assert!(valid_schema_name("work_export"));
        assert!(!valid_schema_name("Work"));
        assert!(!valid_schema_name("a; drop table users"));
        assert!(!valid_schema_name(""));
        assert!(!valid_schema_name("a.b"));
    }
}
