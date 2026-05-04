use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Deserialize, Clone)]
pub struct TemplateManifest {
    pub template: TemplateMeta,

    #[serde(default)]
    pub prompts: Vec<PromptSpec>,

    /// Extra token replacements applied during rendering, evaluated after
    /// all prompts. Keys are literal strings to find; values are template
    /// expressions referencing prompt keys (e.g. `{{gauge_symbol|pascal}}`).
    #[serde(default)]
    pub tokens: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct TemplateMeta {
    pub id: String,
    pub display: String,
    pub description: String,
    /// Free-form category label shown in the picker (e.g. "rust", "js").
    #[serde(default)]
    pub category: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PromptSpec {
    pub key: String,
    pub question: String,

    /// Default value as a template expression. May reference earlier
    /// prompt answers (e.g. `{{project_name|snake}}`).
    #[serde(default)]
    pub default: Option<String>,

    /// One of: `kebab`, `snake`, `pascal`, `identifier`, `none`.
    /// `identifier` permits letters/digits/underscores/hyphens.
    #[serde(default = "default_validate")]
    pub validate: String,
}

fn default_validate() -> String {
    "none".to_string()
}
