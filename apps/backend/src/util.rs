use crate::error::{ApiError, ApiResult};

pub fn slugify(input: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for ch in input.trim().to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    let out = out.trim_matches('-').to_owned();
    if out.is_empty() {
        "item".into()
    } else {
        out.chars().take(80).collect()
    }
}

/// Field slugs end up as JSON keys; keep them to a boring charset.
pub fn field_slug(input: &str) -> String {
    let mut s: String = slugify(input).replace('-', "_");
    if !s.starts_with(|c: char| c.is_ascii_lowercase()) {
        s.insert(0, 'f');
    }
    s.chars().take(60).collect()
}

pub fn clamp_page(page: Option<i64>, limit: Option<i64>) -> ApiResult<(i64, i64)> {
    let page = page.unwrap_or(1);
    let limit = limit.unwrap_or(50);
    if !(1..=1_000_000).contains(&page) || !(1..=200).contains(&limit) {
        return Err(ApiError::BadRequest(
            "page must be >= 1 and limit 1-200".into(),
        ));
    }
    Ok((page, limit))
}

pub fn short(text: &str, max: usize, name: &str) -> ApiResult<()> {
    if text.trim().is_empty() || text.chars().count() > max {
        return Err(ApiError::BadRequest(format!(
            "{name} must be 1-{max} characters"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs() {
        assert_eq!(slugify("  Hello, World! "), "hello-world");
        assert_eq!(slugify("!!!"), "item");
        assert_eq!(field_slug("Due Date"), "due_date");
        assert_eq!(field_slug("1st"), "f1st");
    }

    #[test]
    fn pagination_is_validated() {
        assert!(clamp_page(Some(1), Some(10)).is_ok());
        assert!(clamp_page(Some(0), None).is_err());
        assert!(clamp_page(None, Some(10_000)).is_err());
    }
}
