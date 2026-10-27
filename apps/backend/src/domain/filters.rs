//! Filter engine. The client sends a JSON filter tree; we parse it into a closed AST and compile
//! it to SQL with bound parameters. Nothing from the client is ever concatenated into SQL:
//! operators are a closed enum, column names come from an allow-list, and JSON keys
//! (`data.<slug>`) are bound as parameters after a charset check.

use std::collections::HashMap;

use chrono::{DateTime, Datelike, Duration, TimeZone, Utc};
use serde_json::Value;
use sqlx::{Postgres, QueryBuilder};
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};

const MAX_DEPTH: usize = 6;
const MAX_NODES: usize = 60;
const MAX_LIST: usize = 100;
const MAX_STR: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Eq,
    Ne,
    NeOrNull,
    Gt,
    Lt,
    Gte,
    Lte,
    In,
    Nin,
    Like,
    LikePrefix,
    LikeSuffix,
    Between,
    NBetween,
    IsNull,
    IsNotNull,
}

impl Op {
    fn parse(s: &str) -> Option<Op> {
        Some(match s {
            "eq" => Op::Eq,
            "ne" => Op::Ne,
            "neOrNull" => Op::NeOrNull,
            "gt" => Op::Gt,
            "lt" => Op::Lt,
            "gte" => Op::Gte,
            "lte" => Op::Lte,
            "in" => Op::In,
            "nin" => Op::Nin,
            "like" => Op::Like,
            "like%" => Op::LikePrefix,
            "%like" => Op::LikeSuffix,
            "between" => Op::Between,
            "nbetween" => Op::NBetween,
            "isNull" => Op::IsNull,
            "isNotNull" => Op::IsNotNull,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Data(String),
    CreatedAt,
    UpdatedAt,
    Stage,
    ParentId,
    CardTypeId,
}

#[derive(Debug, Clone)]
pub struct Leaf {
    pub target: Target,
    pub op: Op,
    pub value: Option<Value>,
}

#[derive(Debug, Clone, Default)]
pub struct Group {
    pub and: Vec<Node>,
    pub or: Vec<Node>,
}

#[derive(Debug, Clone)]
pub enum Node {
    Group(Group),
    Leaf(Leaf),
}

fn bad(msg: impl Into<String>) -> ApiError {
    ApiError::BadRequest(format!("invalid filter: {}", msg.into()))
}

fn parse_target(field: &str) -> ApiResult<Target> {
    let f = field.strip_prefix("card.").unwrap_or(field);
    if let Some(slug) = f.strip_prefix("data.") {
        let ok = !slug.is_empty()
            && slug.len() <= 63
            && slug.starts_with(|c: char| c.is_ascii_lowercase())
            && slug
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
        return if ok {
            Ok(Target::Data(slug.to_owned()))
        } else {
            Err(bad("unsafe field name"))
        };
    }
    match f {
        "created_at" | "createdAt" => Ok(Target::CreatedAt),
        "updated_at" | "updatedAt" => Ok(Target::UpdatedAt),
        "stage" | "list_stage_id" => Ok(Target::Stage),
        "parent_id" | "parentId" => Ok(Target::ParentId),
        "card_type_id" => Ok(Target::CardTypeId),
        _ => Err(bad(format!(
            "unknown field `{}`",
            field.chars().take(40).collect::<String>()
        ))),
    }
}

fn parse_node(v: &Value, depth: usize, count: &mut usize) -> ApiResult<Node> {
    *count += 1;
    if *count > MAX_NODES {
        return Err(bad("too many conditions"));
    }
    if depth > MAX_DEPTH {
        return Err(bad("nested too deeply"));
    }
    let obj = v.as_object().ok_or_else(|| bad("expected an object"))?;
    if obj.contains_key("field") {
        if obj
            .keys()
            .any(|k| !matches!(k.as_str(), "field" | "operator" | "value"))
        {
            return Err(bad("unknown key in condition"));
        }
        let field = obj
            .get("field")
            .and_then(Value::as_str)
            .ok_or_else(|| bad("field must be a string"))?;
        let op = obj
            .get("operator")
            .and_then(Value::as_str)
            .and_then(Op::parse)
            .ok_or_else(|| bad("unknown operator"))?;
        let value = obj.get("value").filter(|v| !v.is_null()).cloned();
        if let Some(v) = &value {
            check_value_shape(v)?;
        }
        return Ok(Node::Leaf(Leaf {
            target: parse_target(field)?,
            op,
            value,
        }));
    }
    if obj.keys().any(|k| !matches!(k.as_str(), "and" | "or")) {
        return Err(bad("unknown key in group"));
    }
    let mut g = Group::default();
    for (key, dst) in [("and", &mut g.and), ("or", &mut g.or)] {
        if let Some(arr) = obj.get(key) {
            for child in arr.as_array().ok_or_else(|| bad("and/or must be arrays"))? {
                dst.push(parse_node(child, depth + 1, count)?);
            }
        }
    }
    Ok(Node::Group(g))
}

fn check_value_shape(v: &Value) -> ApiResult<()> {
    match v {
        Value::String(s) if s.chars().count() > MAX_STR => Err(bad("string value too long")),
        Value::Array(a) if a.len() > MAX_LIST => Err(bad("list value too long")),
        Value::Array(a) => a.iter().try_for_each(|x| match x {
            Value::String(s) if s.chars().count() > MAX_STR => Err(bad("string value too long")),
            Value::Array(_) | Value::Object(_) => Err(bad("nested values are not allowed")),
            _ => Ok(()),
        }),
        Value::Object(_) => Err(bad("object values are not allowed")),
        _ => Ok(()),
    }
}

/// Accepts a bare group, `{where: group}` or `{where: {advanced, quick}}` (both are AND-ed).
pub fn parse_root(v: &Value) -> ApiResult<Group> {
    let mut count = 0;
    let inner = match v.get("where") {
        Some(w) => w,
        None => v,
    };
    let nodes: Vec<&Value> = if inner.get("advanced").is_some() || inner.get("quick").is_some() {
        ["advanced", "quick"]
            .iter()
            .filter_map(|k| inner.get(*k))
            .filter(|x| !x.is_null())
            .collect()
    } else {
        vec![inner]
    };
    let mut root = Group::default();
    for n in nodes {
        root.and.push(parse_node(n, 0, &mut count)?);
    }
    Ok(root)
}

/// Structural validation used when a filter is stored (views, groups).
pub fn validate_json(v: &Value) -> ApiResult<()> {
    if v.to_string().len() > 32 * 1024 {
        return Err(bad("filter too large"));
    }
    parse_root(v).map(|_| ())
}

pub struct Ctx<'a> {
    /// slug -> field type, for the workspace being queried. Unknown slugs are rejected.
    pub field_types: &'a HashMap<String, String>,
}

/// Replace date placeholders such as `:startOfDay` (UTC), like the original product did.
pub fn expand_placeholder(s: &str, now: DateTime<Utc>) -> String {
    let day = |d: DateTime<Utc>| {
        Utc.with_ymd_and_hms(d.year(), d.month(), d.day(), 0, 0, 0)
            .unwrap()
    };
    let week =
        |d: DateTime<Utc>| day(d) - Duration::days(d.weekday().num_days_from_sunday() as i64);
    let month = |y: i32, m: i32| {
        let (y, m) = (
            y + (m - 1).div_euclid(12),
            (m - 1).rem_euclid(12) as u32 + 1,
        );
        Utc.with_ymd_and_hms(y, m, 1, 0, 0, 0).unwrap()
    };
    let end = |d: DateTime<Utc>| d - Duration::milliseconds(1);
    let (y, m) = (now.year(), now.month() as i32);
    let v = match s {
        ":startOfDay" => day(now),
        ":endOfDay" => end(day(now) + Duration::days(1)),
        ":startOfYesterday" => day(now) - Duration::days(1),
        ":endOfYesterday" => end(day(now)),
        ":startOfTomorrow" => day(now) + Duration::days(1),
        ":endOfTomorrow" => end(day(now) + Duration::days(2)),
        ":startOfWeek" => week(now),
        ":endOfWeek" => end(week(now) + Duration::days(7)),
        ":startOfLastWeek" => week(now) - Duration::days(7),
        ":endOfLastWeek" => end(week(now)),
        ":startOfNextWeek" => week(now) + Duration::days(7),
        ":endOfNextWeek" => end(week(now) + Duration::days(14)),
        ":startOfMonth" => month(y, m),
        ":endOfMonth" => end(month(y, m + 1)),
        ":startOfLastMonth" => month(y, m - 1),
        ":endOfLastMonth" => end(month(y, m)),
        ":startOfNextMonth" => month(y, m + 1),
        ":endOfNextMonth" => end(month(y, m + 2)),
        ":startOfYear" => month(y, 1),
        ":endOfYear" => end(month(y + 1, 1)),
        ":startOfLastYear" => month(y - 1, 1),
        ":endOfLastYear" => end(month(y, 1)),
        ":startOfNextYear" => month(y + 1, 1),
        ":endOfNextYear" => end(month(y + 2, 1)),
        ":startOfTime" => Utc.timestamp_opt(0, 0).unwrap(),
        ":endOfTime" => Utc.with_ymd_and_hms(2500, 1, 1, 0, 0, 0).unwrap(),
        _ => return s.to_owned(),
    };
    v.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn text_of(v: &Value) -> ApiResult<String> {
    let now = Utc::now();
    match v {
        Value::String(s) => Ok(expand_placeholder(s, now)),
        Value::Number(n) => Ok(n.to_string()),
        Value::Bool(b) => Ok(b.to_string()),
        _ => Err(bad("expected a scalar value")),
    }
}

fn list_of(v: &Value) -> ApiResult<Vec<String>> {
    match v {
        Value::Array(a) if !a.is_empty() => a.iter().map(text_of).collect(),
        Value::Array(_) => Err(bad("empty list")),
        other => Ok(vec![text_of(other)?]),
    }
}

fn escape_like(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

fn cmp_sql(op: Op) -> &'static str {
    match op {
        Op::Eq => "=",
        Op::Ne => "<>",
        Op::Gt => ">",
        Op::Lt => "<",
        Op::Gte => ">=",
        Op::Lte => "<=",
        _ => unreachable!(),
    }
}

enum Kind {
    Num,
    Bool,
    Text,
}

fn data_kind(ty: &str) -> Kind {
    match ty {
        "number" | "percentage" | "currency" => Kind::Num,
        "checkbox" => Kind::Bool,
        _ => Kind::Text,
    }
}

fn push_leaf(qb: &mut QueryBuilder<'_, Postgres>, leaf: &Leaf, ctx: &Ctx) -> ApiResult<()> {
    let need = |v: &Option<Value>| v.clone().ok_or_else(|| bad("this operator needs a value"));
    match &leaf.target {
        Target::Data(slug) => {
            let ty = ctx
                .field_types
                .get(slug)
                .ok_or_else(|| bad(format!("unknown field `{slug}`")))?;
            let kind = data_kind(ty);
            match leaf.op {
                Op::IsNull => {
                    qb.push("(c.data ->> ")
                        .push_bind(slug.clone())
                        .push(") IS NULL");
                }
                Op::IsNotNull => {
                    qb.push("(c.data ->> ")
                        .push_bind(slug.clone())
                        .push(") IS NOT NULL");
                }
                Op::In | Op::Nin => {
                    let vals = list_of(&need(&leaf.value)?)?;
                    if leaf.op == Op::Nin {
                        qb.push("(NOT ");
                    }
                    qb.push("((jsonb_typeof(c.data -> ")
                        .push_bind(slug.clone())
                        .push(") = 'array' AND (c.data -> ")
                        .push_bind(slug.clone())
                        .push(") ?| ")
                        .push_bind(vals.clone())
                        .push("::text[]) OR (jsonb_typeof(c.data -> ")
                        .push_bind(slug.clone())
                        .push(") IS DISTINCT FROM 'array' AND (c.data ->> ")
                        .push_bind(slug.clone())
                        .push(") = ANY(")
                        .push_bind(vals)
                        .push("::text[])))");
                    if leaf.op == Op::Nin {
                        qb.push(" OR (c.data ->> ")
                            .push_bind(slug.clone())
                            .push(") IS NULL)");
                    }
                }
                Op::Like | Op::LikePrefix | Op::LikeSuffix => {
                    let t = escape_like(&text_of(&need(&leaf.value)?)?);
                    let pat = match leaf.op {
                        Op::Like => format!("%{t}%"),
                        Op::LikePrefix => format!("{t}%"),
                        _ => format!("%{t}"),
                    };
                    qb.push("(c.data ->> ")
                        .push_bind(slug.clone())
                        .push(") ILIKE ")
                        .push_bind(pat)
                        .push(" ESCAPE '\\'");
                }
                Op::Between | Op::NBetween => {
                    let v = list_of(&need(&leaf.value)?)?;
                    if v.len() != 2 {
                        return Err(bad("between needs exactly two values"));
                    }
                    if leaf.op == Op::NBetween {
                        qb.push("NOT ");
                    }
                    match kind {
                        Kind::Num => {
                            let (a, b) = (
                                v[0].parse::<f64>().map_err(|_| bad("not a number"))?,
                                v[1].parse::<f64>().map_err(|_| bad("not a number"))?,
                            );
                            qb.push("((c.data ->> ")
                                .push_bind(slug.clone())
                                .push(")::numeric BETWEEN ")
                                .push_bind(a)
                                .push("::float8::numeric AND ")
                                .push_bind(b)
                                .push("::float8::numeric)");
                        }
                        _ => {
                            qb.push("((c.data ->> ")
                                .push_bind(slug.clone())
                                .push(") BETWEEN ")
                                .push_bind(v[0].clone())
                                .push(" AND ")
                                .push_bind(v[1].clone())
                                .push(")");
                        }
                    }
                }
                Op::NeOrNull => {
                    let t = text_of(&need(&leaf.value)?)?;
                    qb.push("((c.data ->> ")
                        .push_bind(slug.clone())
                        .push(") <> ")
                        .push_bind(t)
                        .push(" OR (c.data ->> ")
                        .push_bind(slug.clone())
                        .push(") IS NULL)");
                }
                Op::Eq | Op::Ne | Op::Gt | Op::Lt | Op::Gte | Op::Lte => {
                    let t = text_of(&need(&leaf.value)?)?;
                    let sql = cmp_sql(leaf.op);
                    match kind {
                        Kind::Num => {
                            let n = t.parse::<f64>().map_err(|_| bad("not a number"))?;
                            qb.push("((c.data ->> ")
                                .push_bind(slug.clone())
                                .push(")::numeric ")
                                .push(sql)
                                .push(" ")
                                .push_bind(n)
                                .push("::float8::numeric)");
                        }
                        Kind::Bool => {
                            if !matches!(leaf.op, Op::Eq | Op::Ne) {
                                return Err(bad("checkbox supports eq/ne only"));
                            }
                            let b = t.parse::<bool>().map_err(|_| bad("not a boolean"))?;
                            qb.push("((c.data ->> ")
                                .push_bind(slug.clone())
                                .push(")::boolean ")
                                .push(sql)
                                .push(" ")
                                .push_bind(b)
                                .push(")");
                        }
                        Kind::Text => {
                            qb.push("((c.data ->> ")
                                .push_bind(slug.clone())
                                .push(") ")
                                .push(sql)
                                .push(" ")
                                .push_bind(t)
                                .push(")");
                        }
                    }
                }
            }
        }
        Target::CreatedAt | Target::UpdatedAt => {
            let col = if leaf.target == Target::CreatedAt {
                "c.created_at"
            } else {
                "c.updated_at"
            };
            let ts = |v: &Value| -> ApiResult<DateTime<Utc>> {
                DateTime::parse_from_rfc3339(&text_of(v)?)
                    .map(|d| d.with_timezone(&Utc))
                    .map_err(|_| bad("not an RFC 3339 timestamp"))
            };
            match leaf.op {
                Op::IsNull => {
                    qb.push(format!("{col} IS NULL"));
                }
                Op::IsNotNull => {
                    qb.push(format!("{col} IS NOT NULL"));
                }
                Op::Between | Op::NBetween => {
                    let v = need(&leaf.value)?;
                    let arr = v
                        .as_array()
                        .filter(|a| a.len() == 2)
                        .ok_or_else(|| bad("between needs exactly two values"))?;
                    if leaf.op == Op::NBetween {
                        qb.push("NOT ");
                    }
                    qb.push(format!("({col} BETWEEN "))
                        .push_bind(ts(&arr[0])?)
                        .push(" AND ")
                        .push_bind(ts(&arr[1])?)
                        .push(")");
                }
                Op::Eq | Op::Ne | Op::Gt | Op::Lt | Op::Gte | Op::Lte => {
                    qb.push(format!("({col} {} ", cmp_sql(leaf.op)))
                        .push_bind(ts(&need(&leaf.value)?)?)
                        .push(")");
                }
                _ => return Err(bad("operator not supported for timestamps")),
            }
        }
        Target::Stage | Target::ParentId | Target::CardTypeId => {
            let col = match leaf.target {
                Target::Stage => "cl.list_stage_id",
                Target::ParentId => "c.parent_id",
                _ => "c.card_type_id",
            };
            let ids = |v: &Option<Value>| -> ApiResult<Vec<Uuid>> {
                list_of(&need(v)?)?
                    .iter()
                    .map(|s| Uuid::parse_str(s).map_err(|_| bad("not a valid id")))
                    .collect()
            };
            match leaf.op {
                Op::IsNull => {
                    qb.push(format!("{col} IS NULL"));
                }
                Op::IsNotNull => {
                    qb.push(format!("{col} IS NOT NULL"));
                }
                Op::Eq | Op::In => {
                    qb.push(format!("{col} = ANY("))
                        .push_bind(ids(&leaf.value)?)
                        .push(")");
                }
                Op::Ne | Op::Nin => {
                    qb.push(format!("({col} IS NULL OR NOT {col} = ANY("))
                        .push_bind(ids(&leaf.value)?)
                        .push("))");
                }
                _ => return Err(bad("operator not supported for ids")),
            }
        }
    }
    Ok(())
}

fn push_node(qb: &mut QueryBuilder<'_, Postgres>, node: &Node, ctx: &Ctx) -> ApiResult<()> {
    match node {
        Node::Leaf(l) => push_leaf(qb, l, ctx),
        Node::Group(g) => push_group(qb, g, ctx),
    }
}

/// Emits `( ... )`: all `and` children AND (any of the `or` children).
pub fn push_group(qb: &mut QueryBuilder<'_, Postgres>, g: &Group, ctx: &Ctx) -> ApiResult<()> {
    qb.push("(");
    let mut first = true;
    for n in &g.and {
        if !first {
            qb.push(" AND ");
        }
        first = false;
        push_node(qb, n, ctx)?;
    }
    if !g.or.is_empty() {
        if !first {
            qb.push(" AND ");
        }
        first = false;
        qb.push("(");
        for (i, n) in g.or.iter().enumerate() {
            if i > 0 {
                qb.push(" OR ");
            }
            push_node(qb, n, ctx)?;
        }
        qb.push(")");
    }
    if first {
        qb.push("TRUE");
    }
    qb.push(")");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn types() -> HashMap<String, String> {
        [
            ("title", "text"),
            ("amount", "number"),
            ("done", "checkbox"),
            ("tags", "label"),
        ]
        .into_iter()
        .map(|(a, b)| (a.to_owned(), b.to_owned()))
        .collect()
    }

    fn sql(v: Value) -> ApiResult<String> {
        let t = types();
        let g = parse_root(&v)?;
        let mut qb = QueryBuilder::<Postgres>::new("");
        push_group(&mut qb, &g, &Ctx { field_types: &t })?;
        Ok(qb.sql().to_string())
    }

    #[test]
    fn values_are_bound_never_inlined() {
        let evil = "x'; DROP TABLE cards; --";
        let s = sql(json!({"and":[{"field":"data.title","operator":"eq","value":evil}]})).unwrap();
        assert!(!s.contains("DROP") && !s.contains(evil), "{s}");
        assert!(s.contains("$1") && s.contains("$2"));
    }

    #[test]
    fn rejects_unsafe_and_unknown_fields() {
        assert!(sql(json!({"and":[{"field":"data.title'--","operator":"eq","value":1}]})).is_err());
        assert!(
            sql(json!({"and":[{"field":"users.password","operator":"like","value":"a"}]})).is_err()
        );
        assert!(sql(json!({"and":[{"field":"data.nope","operator":"eq","value":1}]})).is_err());
        assert!(
            sql(json!({"and":[{"field":"data.title","operator":"= 1 OR 1=1","value":1}]})).is_err()
        );
        assert!(
            sql(json!({"and":[{"field":"data.title","operator":"eq","value":{"$ne":1}}]})).is_err()
        );
        assert!(
            sql(json!({"and":[{"field":"data.title","operator":"eq","value":1,"extra":1}]}))
                .is_err()
        );
    }

    #[test]
    fn typed_comparisons() {
        assert!(
            sql(json!({"and":[{"field":"data.amount","operator":"gt","value":"abc"}]})).is_err()
        );
        assert!(
            sql(json!({"and":[{"field":"data.amount","operator":"gt","value":5}]}))
                .unwrap()
                .contains("::numeric")
        );
        assert!(sql(json!({"and":[{"field":"data.done","operator":"gt","value":true}]})).is_err());
        assert!(
            sql(json!({"and":[{"field":"data.tags","operator":"in","value":["a","b"]}]}))
                .unwrap()
                .contains("?|")
        );
    }

    #[test]
    fn like_escapes_wildcards() {
        assert_eq!(escape_like("100%_\\"), "100\\%\\_\\\\");
    }

    #[test]
    fn limits_depth_and_size() {
        let mut v = json!({"and":[{"field":"data.title","operator":"isNull"}]});
        for _ in 0..10 {
            v = json!({"and":[v]});
        }
        assert!(parse_root(&v).is_err());
        let many: Vec<Value> = (0..70)
            .map(|_| json!({"field":"data.title","operator":"isNull"}))
            .collect();
        assert!(parse_root(&json!({"and": many})).is_err());
    }

    #[test]
    fn accepts_view_filter_shapes() {
        let adv = json!({"where":{"advanced":{"and":[{"field":"card.data.title","operator":"isNotNull"}]},"quick":{"or":[{"field":"data.title","operator":"like","value":"a"}]}}});
        assert!(sql(adv).is_ok());
        // An empty root compiles to a TRUE-expression, never to an empty WHERE.
        assert!(sql(json!({})).unwrap().contains("TRUE"));
    }

    #[test]
    fn placeholders() {
        let now = Utc.with_ymd_and_hms(2026, 10, 19, 15, 30, 0).unwrap();
        assert_eq!(
            expand_placeholder(":startOfDay", now),
            "2026-10-19T00:00:00.000Z"
        );
        assert_eq!(
            expand_placeholder(":endOfDay", now),
            "2026-10-19T23:59:59.999Z"
        );
        assert_eq!(
            expand_placeholder(":startOfMonth", now),
            "2026-10-01T00:00:00.000Z"
        );
        assert_eq!(
            expand_placeholder(":endOfYear", now),
            "2026-12-31T23:59:59.999Z"
        );
        assert_eq!(expand_placeholder("plain", now), "plain");
    }
}
