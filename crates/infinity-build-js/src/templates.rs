use infinity_build_core::{BuildError, BuildResult};
use std::collections::HashMap;
use std::path::Path;

/// Default HTML template for both React and BaseInstrument
/// instruments.
pub const DEFAULT_HTML_TEMPLATE: &str = include_str!("templates/instrument.html");

/// Default JS harness for React instruments

pub const DEFAULT_JS_HARNESS_TEMPLATE: &str = include_str!("templates/instrument.cjs");

#[derive(Debug, Default)]
pub struct TemplateContext {
    pub vars: HashMap<String, String>,
    pub lists: HashMap<String, Vec<String>>,
}

impl TemplateContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn var(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.vars.insert(key.into(), value.into());
        self
    }

    pub fn list(mut self, key: impl Into<String>, value: Vec<String>) -> Self {
        self.lists.insert(key.into(), value);
        self
    }
}

pub fn render(template: &str, ctx: &TemplateContext) -> BuildResult<String> {
    let after_sections = render_sections(template, ctx)?;
    Ok(render_vars(&after_sections, ctx))
}

fn render_sections(template: &str, ctx: &TemplateContext) -> BuildResult<String> {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;

    while let Some(open_start) = rest.find("{{#") {
        out.push_str(&rest[..open_start]);

        let after_open = &rest[open_start + 3..];
        let open_end = after_open.find("}}").ok_or_else(|| {
            BuildError::backend_failure("template-render", "unterminated `{{#...}}` section opener")
        })?;
        let key = after_open[..open_end].trim().to_string();
        let body_start = open_start + 3 + open_end + 2;
        let close_marker = format!("{{{{/{key}}}}}");
        let body = &rest[body_start..];
        let close_rel = body.find(&close_marker).ok_or_else(|| {
            BuildError::backend_failure(
                "template-render",
                format!("unterminated section `{{{{#{key}}}}}`"),
            )
        })?;
        let body_text = &body[..close_rel];

        if let Some(items) = ctx.lists.get(&key) {
            for item in items {
                let mut item_ctx = TemplateContext::new();
                item_ctx.vars.extend(ctx.vars.clone());
                item_ctx.vars.insert("this".to_string(), item.clone());
                out.push_str(&render_vars(body_text, &item_ctx));
            }
        }

        // Continue past the close tag.
        rest = &body[close_rel + close_marker.len()..];
    }

    out.push_str(rest);
    Ok(out)
}

fn render_vars(template: &str, ctx: &TemplateContext) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;

    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            // Malformed `{{` with no matching `}}`. Preserve verbatim
            out.push_str(&rest[start..]);
            return out;
        };
        let key = after[..end].trim();
        if key.starts_with('#') || key.starts_with('/') {
            out.push_str(&rest[start..start + 2 + end + 2]);
        } else if let Some(value) = ctx.vars.get(key) {
            out.push_str(value);
        }
        rest = &after[end + 2..];
    }

    out.push_str(rest);
    out
}

pub fn load_template(path: Option<&Path>, default: &'static str) -> BuildResult<String> {
    match path {
        Some(p) => std::fs::read_to_string(p).map_err(|e| BuildError::io(p, e)),
        None => Ok(default.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_simple_vars() {
        let ctx = TemplateContext::new()
            .var("name", "Alice")
            .var("greeting", "Hello");
        let out = render("{{ greeting }}, {{ name }}!", &ctx).unwrap();
        assert_eq!(out, "Hello, Alice!");
    }

    #[test]
    fn renders_list_section() {
        let ctx =
            TemplateContext::new().list("imports", vec!["/JS/a.js".into(), "/JS/b.js".into()]);
        let out = render(
            "before\n{{#imports}}<i>{{ this }}</i>\n{{/imports}}after",
            &ctx,
        )
        .unwrap();
        assert_eq!(out, "before\n<i>/JS/a.js</i>\n<i>/JS/b.js</i>\nafter");
    }

    #[test]
    fn empty_list_produces_no_output() {
        let ctx = TemplateContext::new();
        let out = render("a {{#xs}}item{{/xs}} z", &ctx).unwrap();
        assert_eq!(out, "a  z");
    }

    #[test]
    fn missing_var_is_empty() {
        let ctx = TemplateContext::new().var("a", "1");
        let out = render("{{ a }}/{{ b }}", &ctx).unwrap();
        assert_eq!(out, "1/");
    }

    #[test]
    fn renders_default_html_template() {
        let ctx = TemplateContext::new()
            .var("templateId", "PFD")
            .var("mountElementId", "MSFS_REACT_MOUNT")
            .var("instrumentPath", "/Pages/.../instrument.index.js")
            .var("cssPath", "/Pages/.../instrument.css")
            .list(
                "imports",
                vec!["/JS/dataStorage.js".into(), "/JS/extra.js".into()],
            );
        let out = render(DEFAULT_HTML_TEMPLATE, &ctx).unwrap();
        assert!(out.contains(r#"id="PFD""#));
        assert!(out.contains(r#"id="MSFS_REACT_MOUNT""#));
        assert!(out.contains(r#"import-script="/JS/dataStorage.js""#));
        assert!(out.contains(r#"import-script="/JS/extra.js""#));
        assert!(out.contains(r#"import-script="/Pages/.../instrument.index.js""#));
        assert!(out.contains(r#"href="/Pages/.../instrument.css""#));
    }

    #[test]
    fn renders_default_js_harness() {
        let ctx = TemplateContext::new()
            .var("templateId", "PFD")
            .var("instrumentName", "infinity-bridge-pfd")
            .var("jsPath", "/Pages/.../instrument.js")
            .var("isInteractive", "true");
        let out = render(DEFAULT_JS_HARNESS_TEMPLATE, &ctx).unwrap();
        assert!(out.contains("class _MachInstrument_PFD"));
        assert!(out.contains(r#"return 'PFD';"#));
        assert!(out.contains(r#"Include.addScript("/Pages/.../instrument.js");"#));
        assert!(out.contains(r#"registerInstrument("infinity-bridge-pfd""#));
        assert!(out.contains("return true;"));
    }
}
