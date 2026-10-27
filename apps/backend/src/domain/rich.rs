//! Rich text is stored as a ProseMirror/TipTap document (JSON). The server never trusts its shape:
//! only the nodes, marks and attributes the editor offers are accepted, links may only point at
//! `http(s)` or `mailto`, and size and depth are bounded. Clients render the document themselves
//! from this structure (no HTML is stored or injected anywhere), so an accepted document cannot carry
//! script. Plain strings stay valid for content written before the editor existed.

use serde_json::Value;

const MAX_BYTES: usize = 200_000;
const MAX_DEPTH: usize = 12;
const MAX_NODES: usize = 5_000;
const MAX_TEXT: usize = 100_000;

/// node type -> allowed `attrs` keys
fn node_attrs(ty: &str) -> Option<&'static [&'static str]> {
    Some(match ty {
        "doc" | "bulletList" | "listItem" | "blockquote" | "hardBreak" | "horizontalRule"
        | "text" => &[],
        "paragraph" => &["textAlign"],
        "heading" => &["level", "textAlign"],
        "orderedList" => &["start", "type"],
        "codeBlock" => &["language"],
        _ => return None,
    })
}

fn mark_attrs(ty: &str) -> Option<&'static [&'static str]> {
    Some(match ty {
        "bold" | "italic" | "strike" | "code" | "underline" => &[],
        "link" => &["href", "target", "rel", "class", "title"],
        _ => return None,
    })
}

fn only_keys(obj: &serde_json::Map<String, Value>, allowed: &[&str]) -> bool {
    obj.keys().all(|k| allowed.contains(&k.as_str()))
}

fn safe_href(href: &str) -> bool {
    let h = href.trim();
    h.len() <= 2048
        && !h.chars().any(|c| c.is_control())
        && (h.starts_with("https://") || h.starts_with("http://") || h.starts_with("mailto:"))
}

fn check_attrs(attrs: Option<&Value>, allowed: &[&str], ty: &str) -> Result<(), String> {
    let Some(attrs) = attrs else { return Ok(()) };
    if attrs.is_null() {
        return Ok(());
    }
    let obj = attrs
        .as_object()
        .ok_or_else(|| format!("{ty}: attrs must be an object"))?;
    if !only_keys(obj, allowed) {
        return Err(format!("{ty}: unknown attribute"));
    }
    for (k, v) in obj {
        let ok = match (ty, k.as_str()) {
            ("heading", "level") => v.as_i64().is_some_and(|l| (1..=3).contains(&l)),
            ("orderedList", "start") => v.as_i64().is_some_and(|n| (0..=100_000).contains(&n)),
            ("orderedList", "type") | ("link", "rel") | ("link", "class") | ("link", "title") => {
                v.is_null() || v.as_str().is_some_and(|s| s.len() <= 100)
            }
            ("codeBlock", "language") => {
                v.is_null()
                    || v.as_str().is_some_and(|s| {
                        s.len() <= 32
                            && s.chars()
                                .all(|c| c.is_ascii_alphanumeric() || "+-_#.".contains(c))
                    })
            }
            ("paragraph" | "heading", "textAlign") => {
                v.is_null() || matches!(v.as_str(), Some("left" | "center" | "right" | "justify"))
            }
            ("link", "href") => v.as_str().is_some_and(safe_href),
            ("link", "target") => v.is_null() || matches!(v.as_str(), Some("_blank" | "_self")),
            _ => false,
        };
        if !ok {
            return Err(format!("{ty}: invalid value for {k}"));
        }
    }
    Ok(())
}

struct Walk {
    nodes: usize,
    text: usize,
}

fn walk(v: &Value, depth: usize, w: &mut Walk, top: bool) -> Result<(), String> {
    if depth > MAX_DEPTH {
        return Err("document is nested too deeply".into());
    }
    w.nodes += 1;
    if w.nodes > MAX_NODES {
        return Err("document has too many nodes".into());
    }
    let obj = v.as_object().ok_or("a node must be an object")?;
    if !only_keys(obj, &["type", "attrs", "content", "marks", "text"]) {
        return Err("unknown node property".into());
    }
    let ty = obj
        .get("type")
        .and_then(Value::as_str)
        .ok_or("a node needs a type")?;
    if top != (ty == "doc") {
        return Err("`doc` must be the root and only the root".into());
    }
    let allowed = node_attrs(ty).ok_or_else(|| format!("node type `{ty}` is not allowed"))?;
    check_attrs(obj.get("attrs"), allowed, ty)?;
    if ty == "text" {
        let text = obj
            .get("text")
            .and_then(Value::as_str)
            .ok_or("a text node needs text")?;
        w.text += text.chars().count();
        if w.text > MAX_TEXT {
            return Err("document has too much text".into());
        }
        if obj.contains_key("content") {
            return Err("a text node has no content".into());
        }
    } else if obj.contains_key("text") {
        return Err("only text nodes carry text".into());
    }
    if let Some(marks) = obj.get("marks") {
        if ty != "text" {
            return Err("only text can be marked".into());
        }
        for m in marks.as_array().ok_or("marks must be a list")? {
            let mo = m.as_object().ok_or("a mark must be an object")?;
            if !only_keys(mo, &["type", "attrs"]) {
                return Err("unknown mark property".into());
            }
            let mt = mo
                .get("type")
                .and_then(Value::as_str)
                .ok_or("a mark needs a type")?;
            let allowed = mark_attrs(mt).ok_or_else(|| format!("mark `{mt}` is not allowed"))?;
            check_attrs(mo.get("attrs"), allowed, mt)?;
        }
    }
    if let Some(content) = obj.get("content") {
        for child in content.as_array().ok_or("content must be a list")? {
            walk(child, depth + 1, w, false)?;
        }
    }
    Ok(())
}

/// `Ok` for a document the editor could have produced.
pub fn validate_doc(v: &Value) -> Result<(), String> {
    if v.to_string().len() > MAX_BYTES {
        return Err("document is too large".into());
    }
    walk(v, 0, &mut Walk { nodes: 0, text: 0 }, true)
}

/// A rich-text field value: legacy plain text or a valid document.
pub fn validate_value(v: &Value) -> Result<(), String> {
    match v {
        Value::String(s) if s.len() <= MAX_BYTES => Ok(()),
        Value::String(_) => Err("text is too large".into()),
        Value::Object(_) => validate_doc(v),
        _ => Err("expected text or a document".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn doc(content: Value) -> Value {
        json!({"type": "doc", "content": content})
    }

    #[test]
    fn accepts_what_the_editor_produces() {
        let d = doc(json!([
            {"type": "heading", "attrs": {"level": 2}, "content": [{"type": "text", "text": "Title"}]},
            {"type": "paragraph", "content": [
                {"type": "text", "text": "bold ", "marks": [{"type": "bold"}]},
                {"type": "text", "text": "link", "marks": [{"type": "link", "attrs": {"href": "https://example.test/a?b=1", "target": "_blank", "rel": "noopener noreferrer nofollow", "class": null, "title": null}}]},
                {"type": "hardBreak"},
                {"type": "text", "text": "mail", "marks": [{"type": "link", "attrs": {"href": "mailto:a@example.test"}}]}
            ]},
            {"type": "bulletList", "content": [{"type": "listItem", "content": [{"type": "paragraph", "content": [{"type": "text", "text": "one"}]}]}]},
            {"type": "orderedList", "attrs": {"start": 1, "type": null}, "content": [{"type": "listItem", "content": [{"type": "paragraph"}]}]},
            {"type": "codeBlock", "attrs": {"language": "rust"}, "content": [{"type": "text", "text": "fn main() {}"}]},
            {"type": "blockquote", "content": [{"type": "paragraph"}]},
            {"type": "horizontalRule"}
        ]));
        assert_eq!(validate_doc(&d), Ok(()));
        assert_eq!(validate_value(&json!("legacy plain text")), Ok(()));
    }

    #[test]
    fn rejects_active_and_unknown_content() {
        let bad = [
            doc(
                json!([{"type": "paragraph", "content": [{"type": "text", "text": "x", "marks": [{"type": "link", "attrs": {"href": "javascript:alert(1)"}}]}]}]),
            ),
            doc(
                json!([{"type": "paragraph", "content": [{"type": "text", "text": "x", "marks": [{"type": "link", "attrs": {"href": " JaVaScRiPt:alert(1)"}}]}]}]),
            ),
            doc(
                json!([{"type": "paragraph", "content": [{"type": "text", "text": "x", "marks": [{"type": "link", "attrs": {"href": "data:text/html,<script>"}}]}]}]),
            ),
            doc(json!([{"type": "script", "content": []}])),
            doc(json!([{"type": "image", "attrs": {"src": "https://example.test/x.png"}}])),
            doc(json!([{"type": "paragraph", "attrs": {"onclick": "alert(1)"}}])),
            doc(
                json!([{"type": "paragraph", "content": [{"type": "text", "text": "x", "marks": [{"type": "style", "attrs": {"css": "x"}}]}]}]),
            ),
            doc(json!([{"type": "heading", "attrs": {"level": 9}}])),
            doc(json!([{"type": "paragraph", "html": "<b>x</b>"}])),
            doc(json!([{"type": "doc"}])),
            json!({"type": "paragraph"}),
            json!({"type": "doc", "content": "text"}),
            json!([1, 2]),
        ];
        for d in bad {
            assert!(validate_doc(&d).is_err(), "{d}");
        }
        assert!(validate_value(&json!(5)).is_err());
        assert!(validate_value(&json!(null)).is_err());
    }

    #[test]
    fn bounds_depth_size_and_node_count() {
        let mut deep = json!({"type": "paragraph"});
        for _ in 0..20 {
            deep = json!({"type": "blockquote", "content": [deep]});
        }
        assert!(validate_doc(&doc(json!([deep])))
            .unwrap_err()
            .contains("deeply"));
        let many: Vec<Value> = (0..6000).map(|_| json!({"type": "paragraph"})).collect();
        assert!(validate_doc(&doc(json!(many)))
            .unwrap_err()
            .contains("too many"));
        let big = "x".repeat(250_000);
        assert!(validate_doc(&doc(
            json!([{"type": "paragraph", "content": [{"type": "text", "text": big}]}])
        ))
        .is_err());
    }
}
