use anyhow::{Context, Result};
use infinity_build_js::JsBuildConfig;
use infinity_build_package::SimPackage;
use infinity_build_rust::RustConfig;
use serde::Deserialize;
use std::{fs, path::Path};

#[derive(Debug, Deserialize, Default, Clone)]
pub struct InfinityMsfsToml {
    /// Cargo build pipeline. Holds shared defaults plus the
    /// `[[rust.packages]]` list consumed by `infinity-msfs build`.
    #[serde(default)]
    pub rust: RustConfig,

    /// JS/TS instrument bundling. Built by the JS half of `infinity-msfs build`.
    #[serde(default)]
    pub js: Option<JsBuildConfig>,

    /// MSFS sim packages compiled by `fspackagetool.exe`. Independent
    /// from `[rust]` and only consumed by `infinity-msfs package`.
    /// Windows + an installed sim required.
    #[serde(default)]
    pub sim_packages: Vec<SimPackage>,

    #[serde(default)]
    pub hooks: HooksConfig,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct HooksConfig {
    #[serde(default)]
    pub pre: Vec<String>,

    #[serde(default)]
    pub post: Vec<String>,
}

impl InfinityMsfsToml {
    pub fn load(path: &Path) -> Result<Self> {
        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read config file {}", path.display()))?;
        toml::from_str(&raw)
            .with_context(|| format!("failed to parse TOML in {}", path.display()))
    }
}
