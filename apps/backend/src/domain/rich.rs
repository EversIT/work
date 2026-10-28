//! Rich text is stored as a ProseMirror/TipTap document (JSON). The server never trusts its shape:
//! only the nodes, marks and attributes the editor offers are accepted, links may only point at
//! `http(s)` or `mailto`, and size and depth are bounded. Clients render the document themselves
//! from this structure (no HTML is stored or injected anywhere), so an accepted document cannot carry
//! script. Plain strings stay valid for content written before the editor existed.

use serde_json::{json, Value};

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

// ---- plain text and Markdown <-> document ---------------------------------------------------
//
// Machines (the MCP server, the AI assistant) talk Markdown, the editor stores a document. These
// two functions translate between them for the part of Markdown the editor can show. Everything
// `from_markdown` returns has passed `validate_doc`.

/// The text of a rich-text value as Markdown. Legacy plain text comes back unchanged.
pub fn to_markdown(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Object(_) => blocks_to_md(v, 0).trim_end().to_owned(),
        _ => String::new(),
    }
}

fn inline_to_md(nodes: &[Value]) -> String {
    let mut out = String::new();
    for n in nodes {
        match n.get("type").and_then(Value::as_str) {
            Some("text") => {
                let mut t = n
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                for m in n
                    .get("marks")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    t = match m.get("type").and_then(Value::as_str) {
                        Some("bold") => format!("**{t}**"),
                        Some("italic") => format!("_{t}_"),
                        Some("strike") => format!("~~{t}~~"),
                        Some("code") => format!("`{t}`"),
                        Some("link") => {
                            let href = m
                                .pointer("/attrs/href")
                                .and_then(Value::as_str)
                                .unwrap_or_default();
                            format!("[{t}]({href})")
                        }
                        _ => t,
                    };
                }
                out.push_str(&t);
            }
            Some("hardBreak") => out.push('\n'),
            _ => {}
        }
    }
    out
}

fn children(v: &Value) -> &[Value] {
    v.get("content")
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

fn blocks_to_md(node: &Value, depth: usize) -> String {
    let pad = "  ".repeat(depth);
    let mut blocks: Vec<String> = vec![];
    for b in children(node) {
        let text = match b.get("type").and_then(Value::as_str) {
            Some("paragraph") => inline_to_md(children(b)),
            Some("heading") => {
                let level = b
                    .pointer("/attrs/level")
                    .and_then(Value::as_u64)
                    .unwrap_or(1);
                format!(
                    "{} {}",
                    "#".repeat(level.clamp(1, 3) as usize),
                    inline_to_md(children(b))
                )
            }
            Some(kind @ ("bulletList" | "orderedList")) => {
                let start = b
                    .pointer("/attrs/start")
                    .and_then(Value::as_u64)
                    .unwrap_or(1);
                children(b)
                    .iter()
                    .enumerate()
                    .map(|(i, item)| {
                        let marker = if kind == "bulletList" {
                            "-".to_owned()
                        } else {
                            format!("{}.", start + i as u64)
                        };
                        let body = blocks_to_md(item, depth + 1);
                        let mut lines = body.trim_end().lines();
                        let first = lines.next().unwrap_or_default().trim_start().to_owned();
                        let rest: String = lines.map(|l| format!("\n{l}")).collect();
                        format!("{pad}{marker} {first}{rest}")
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            }
            Some("blockquote") => blocks_to_md(b, depth)
                .trim_end()
                .lines()
                .map(|l| format!("> {l}"))
                .collect::<Vec<_>>()
                .join("\n"),
            Some("codeBlock") => {
                let lang = b
                    .pointer("/attrs/language")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let code: String = children(b)
                    .iter()
                    .filter_map(|t| t.get("text").and_then(Value::as_str))
                    .collect();
                format!("```{lang}\n{code}\n```")
            }
            Some("horizontalRule") => "---".to_owned(),
            _ => continue,
        };
        blocks.push(text);
    }
    // Inside a list item the blocks sit on consecutive lines; at the top they are paragraphs apart.
    blocks.join(if depth == 0 { "\n\n" } else { "\n" })
}

/// Lines longer than this stay plain text: the inline scan below looks ahead for closing marks,
/// and bounding the line keeps that cheap whatever a client sends.
const MAX_INLINE_LINE: usize = 1_000;

/// Marks of the inline Markdown we read: `**bold**`, `` `code` `` and `[text](https://...)`.
fn inline_from_md(line: &str) -> Vec<Value> {
    if line.len() > MAX_INLINE_LINE {
        return vec![json!({"type": "text", "text": line})];
    }
    let mut out: Vec<Value> = vec![];
    let mut plain = String::new();
    let flush = |plain: &mut String, out: &mut Vec<Value>| {
        if !plain.is_empty() {
            out.push(json!({"type": "text", "text": std::mem::take(plain)}));
        }
    };
    let mut i = 0; // byte offset, always on a char boundary
    while i < line.len() {
        let rest = &line[i..];
        let marked = if let Some((inner, _)) =
            rest.strip_prefix("**").and_then(|r| r.split_once("**"))
        {
            Some((inner, json!({"type": "bold"}), inner.len() + 4))
        } else if let Some((inner, _)) = rest.strip_prefix('`').and_then(|r| r.split_once('`')) {
            Some((inner, json!({"type": "code"}), inner.len() + 2))
        } else {
            None
        };
        if let Some((inner, mark, len)) = marked.filter(|(inner, ..)| !inner.is_empty()) {
            flush(&mut plain, &mut out);
            out.push(json!({"type": "text", "text": inner, "marks": [mark]}));
            i += len;
            continue;
        }
        let link = rest
            .strip_prefix('[')
            .and_then(|r| r.split_once("]("))
            .and_then(|(label, after)| after.split_once(')').map(|(href, _)| (label, href)))
            .filter(|(label, href)| !label.is_empty() && !label.contains('[') && safe_href(href));
        if let Some((label, href)) = link {
            flush(&mut plain, &mut out);
            out.push(json!({"type": "text", "text": label,
                "marks": [{"type": "link", "attrs": {"href": href.trim()}}]}));
            i += label.len() + href.len() + 4;
            continue;
        }
        let ch = rest.chars().next().expect("i < len");
        plain.push(ch);
        i += ch.len_utf8();
    }
    flush(&mut plain, &mut out);
    out
}

fn paragraph(lines: &[&str]) -> Value {
    let mut content = vec![];
    for (i, l) in lines.iter().enumerate() {
        if i > 0 {
            content.push(json!({"type": "hardBreak"}));
        }
        content.extend(inline_from_md(l));
    }
    if content.is_empty() {
        json!({"type": "paragraph"})
    } else {
        json!({"type": "paragraph", "content": content})
    }
}

fn list_marker(line: &str) -> Option<(bool, &str)> {
    let t = line.trim_start();
    if let Some(r) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
        return Some((false, r));
    }
    let digits = t.bytes().take_while(u8::is_ascii_digit).count();
    if digits > 0 && digits <= 4 {
        if let Some(r) = t[digits..].strip_prefix(". ") {
            return Some((true, r));
        }
    }
    None
}

/// Turns Markdown (headings, lists, quotes, code fences, bold, code, links) into a document the
/// editor shows. Anything else stays text. The result is validated; `Err` means it was too large.
pub fn from_markdown(text: &str) -> Result<Value, String> {
    if text.len() > MAX_BYTES {
        return Err("text is too large".into());
    }
    let lines: Vec<&str> = text.lines().collect();
    let mut blocks: Vec<Value> = vec![];
    let mut para: Vec<&str> = vec![];
    let mut i = 0;
    macro_rules! flush_para {
        () => {
            if !para.is_empty() {
                blocks.push(paragraph(&para));
                para.clear();
            }
        };
    }
    while i < lines.len() {
        let line = lines[i];
        let t = line.trim();
        if t.is_empty() {
            flush_para!();
            i += 1;
        } else if let Some(lang) = t.strip_prefix("```") {
            flush_para!();
            let lang = lang.trim();
            let mut code = vec![];
            i += 1;
            while i < lines.len() && !lines[i].trim().starts_with("```") {
                code.push(lines[i]);
                i += 1;
            }
            i += 1; // the closing fence
            let mut node = json!({"type": "codeBlock"});
            if !lang.is_empty()
                && lang.len() <= 32
                && lang
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "+-_#.".contains(c))
            {
                node["attrs"] = json!({"language": lang});
            }
            if !code.is_empty() {
                node["content"] = json!([{"type": "text", "text": code.join("\n")}]);
            }
            blocks.push(node);
        } else if t.starts_with('#') && t.trim_start_matches('#').starts_with(' ') {
            flush_para!();
            let level = t.chars().take_while(|c| *c == '#').count().min(3);
            blocks.push(json!({"type": "heading", "attrs": {"level": level},
                "content": inline_from_md(t.trim_start_matches('#').trim())}));
            i += 1;
        } else if let Some((ordered, _)) = list_marker(line) {
            flush_para!();
            let mut items = vec![];
            while i < lines.len() {
                match list_marker(lines[i]) {
                    Some((o, item)) if o == ordered => {
                        items.push(json!({"type": "listItem", "content": [paragraph(&[item])]}));
                        i += 1;
                    }
                    _ => break,
                }
            }
            blocks.push(if ordered {
                json!({"type": "orderedList", "attrs": {"start": 1}, "content": items})
            } else {
                json!({"type": "bulletList", "content": items})
            });
        } else if let Some(q) = t.strip_prefix("> ").or_else(|| t.strip_prefix('>')) {
            flush_para!();
            let mut quoted = vec![q.trim()];
            i += 1;
            while i < lines.len() {
                match lines[i].trim().strip_prefix('>') {
                    Some(r) => {
                        quoted.push(r.trim());
                        i += 1;
                    }
                    None => break,
                }
            }
            blocks.push(json!({"type": "blockquote", "content": [paragraph(&quoted)]}));
        } else if matches!(t, "---" | "***" | "___") {
            flush_para!();
            blocks.push(json!({"type": "horizontalRule"}));
            i += 1;
        } else {
            para.push(t);
            i += 1;
        }
    }
    flush_para!();
    if blocks.is_empty() {
        blocks.push(json!({"type": "paragraph"}));
    }
    let doc = json!({"type": "doc", "content": blocks});
    validate_doc(&doc)?;
    Ok(doc)
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

    #[test]
    fn markdown_roundtrips_for_what_the_editor_shows() {
        let md = "# Title\n\nSome **bold** text with `code` and a [link](https://example.test).\n\n- one\n- two\n\n1. first\n2. second\n\n> quoted\n\n```rust\nfn main() {}\n```";
        let d = from_markdown(md).unwrap();
        assert_eq!(validate_doc(&d), Ok(()));
        assert_eq!(to_markdown(&d), md);
    }

    #[test]
    fn markdown_never_yields_unsafe_links_or_unknown_nodes() {
        let d = from_markdown(
            "[x](javascript:alert(1)) and <script>alert(1)</script> ![i](https://x.test/a.png)",
        )
        .unwrap();
        assert_eq!(validate_doc(&d), Ok(()));
        // It stays visible as text, but never becomes a link; an image becomes a plain https link.
        assert!(!d.to_string().contains("\"href\":\"javascript"), "{d}");
        assert!(
            d.to_string().contains("\"href\":\"https://x.test/a.png\""),
            "{d}"
        );
        assert_eq!(to_markdown(&json!("legacy")), "legacy");
        assert_eq!(to_markdown(&json!(null)), "");
        assert!(from_markdown(&"x".repeat(250_000)).is_err());
        assert_eq!(validate_doc(&from_markdown("").unwrap()), Ok(()));
    }
}
