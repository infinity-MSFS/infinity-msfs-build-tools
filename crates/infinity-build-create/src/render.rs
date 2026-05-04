use anyhow::{Result, bail};
use heck::{ToKebabCase, ToPascalCase, ToShoutySnakeCase, ToSnakeCase};
use std::collections::BTreeMap;

/// Resolve `{{key}}` and `{{key|filter}}` references against `vars`.
/// Filters: `kebab`, `snake`, `pascal`, `upper`. Unknown filters error.
pub fn render_expr(expr: &str, vars: &BTreeMap<String, String>) -> Result<String> {
    let mut out = String::with_capacity(expr.len());
    let mut rest = expr;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            bail!("unterminated `{{{{` in template expression: {expr}");
        };
        let inner = after[..end].trim();
        let (key, filter) = match inner.split_once('|') {
            Some((k, f)) => (k.trim(), Some(f.trim())),
            None => (inner, None),
        };
        let value = vars
            .get(key)
            .cloned()
            .unwrap_or_else(|| panic!("unknown template variable `{key}`"));
        out.push_str(&apply_filter(&value, filter)?);
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    Ok(out)
}

fn apply_filter(value: &str, filter: Option<&str>) -> Result<String> {
    Ok(match filter {
        None => value.to_string(),
        Some("kebab") => value.to_kebab_case(),
        Some("snake") => value.to_snake_case(),
        Some("pascal") => value.to_pascal_case(),
        Some("upper") => value.to_shouty_snake_case(),
        Some(other) => bail!("unknown filter `{other}` in template expression"),
    })
}

/// Apply token substitutions to a file's content. Substitutions run in
/// length-descending order so longer tokens (e.g. `replace-me-foo`)
/// match before shorter prefixes (`replace-me`).
pub fn apply_tokens(content: &str, tokens: &BTreeMap<String, String>) -> String {
    let mut keys: Vec<&String> = tokens.keys().collect();
    keys.sort_by_key(|k| std::cmp::Reverse(k.len()));
    let mut out = content.to_string();
    for k in keys {
        out = out.replace(k, &tokens[k]);
    }
    out
}

/// Substitute tokens in a path. Same logic as content; runs on each
/// path component so directory names like `replace-me/` get rewritten.
pub fn apply_tokens_path(path: &str, tokens: &BTreeMap<String, String>) -> String {
    apply_tokens(path, tokens)
}

pub fn validate(value: &str, kind: &str) -> Result<()> {
    let ok = match kind {
        "none" => true,
        "kebab" => is_kebab(value),
        "snake" => is_snake(value),
        "pascal" => is_pascal(value),
        "identifier" => is_identifier(value),
        other => bail!("unknown validator `{other}`"),
    };
    if !ok {
        bail!("`{value}` is not valid {kind}-case");
    }
    Ok(())
}

fn is_kebab(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !s.starts_with('-')
        && !s.ends_with('-')
}

fn is_snake(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        && !s.starts_with('_')
        && !s.ends_with('_')
}

fn is_pascal(s: &str) -> bool {
    let first = s.chars().next();
    matches!(first, Some(c) if c.is_ascii_uppercase())
        && s.chars().all(|c| c.is_ascii_alphanumeric())
}

fn is_identifier(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}
