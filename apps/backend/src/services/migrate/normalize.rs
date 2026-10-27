//! Small, pure conversions from source values to what this server accepts.

use serde_json::{json, Value};

pub(super) fn clip(s: &str, max: usize) -> String {
    s.trim().chars().take(max).collect()
}

pub(super) fn normalize_role(raw: &str) -> Option<&'static str> {
    match raw.trim().to_lowercase().as_str() {
        "owner" => Some("owner"),
        "admin" | "administrator" => Some("admin"),
        "member" | "user" | "editor" => Some("member"),
        "viewer" | "guest" | "readonly" | "read_only" => Some("viewer"),
        _ => None,
    }
}

/// Only bcrypt and Argon2 hashes are accepted; everything else becomes "no password".
pub(super) fn acceptable_hash(h: &str) -> &str {
    let ok = [
        "$2a$",
        "$2b$",
        "$2x$",
        "$2y$",
        "$argon2id$",
        "$argon2i$",
        "$argon2d$",
    ]
    .iter()
    .any(|p| h.starts_with(p));
    if ok && h.len() <= 255 {
        h
    } else {
        ""
    }
}

/// Dropdown/label items become `[{"item": "..."}]` whether they come as strings or objects.
pub(super) fn normalize_items(items: &Value) -> Option<Value> {
    let list = items.as_array()?;
    let out: Vec<Value> = list
        .iter()
        .filter_map(|i| match i {
            Value::String(s) => Some(json!({"item": s})),
            Value::Object(o) if o.get("item").is_some_and(Value::is_string) => Some(i.clone()),
            _ => None,
        })
        .collect();
    (!out.is_empty()).then_some(Value::Array(out))
}

pub(super) fn slugify(name: &str) -> String {
    crate::util::field_slug(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles_hashes_and_items_are_normalized() {
        assert_eq!(normalize_role(" Admin "), Some("admin"));
        assert_eq!(normalize_role("guest"), Some("viewer"));
        assert_eq!(normalize_role("root"), None);
        assert_eq!(
            acceptable_hash("$2b$10$abcdefghijklmnopqrstuv"),
            "$2b$10$abcdefghijklmnopqrstuv"
        );
        assert_eq!(acceptable_hash("plain-text-password"), "");
        assert_eq!(acceptable_hash(""), "");
        assert_eq!(
            normalize_items(&json!(["High", {"item": "Low"}, 5, {"x": 1}])),
            Some(json!([{"item": "High"}, {"item": "Low"}]))
        );
        assert_eq!(normalize_items(&json!([])), None);
        assert_eq!(normalize_items(&json!("nope")), None);
    }
}
