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

    /// infinity-aegis DRM release wiring. When enabled, the build id is
    /// exported into the environment *before* cargo runs so the
    /// `#[protected]` manifests, the client-pinned build id, and the
    /// post-build seal/upload all agree. The seal+upload itself runs from a
    /// `[hooks].post` entry (e.g. `pwsh -File scripts/aegis-release.ps1
    /// -SkipBuild`), which inherits the exported id.
    #[serde(default)]
    pub aegis: AegisConfig,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct AegisConfig {
    /// Turn on aegis build-id injection.
    #[serde(default)]
    pub enabled: bool,

    /// Explicit build id. When omitted, derived as
    /// `<build_id_prefix>-<git short sha>` (plus `-dirty` if the working
    /// tree has uncommitted changes).
    #[serde(default)]
    pub build_id: Option<String>,

    /// Prefix for the auto-derived build id. Default `"build"`.
    #[serde(default = "default_aegis_prefix")]
    pub build_id_prefix: String,

    /// Environment variables set to the build id before the build. Default:
    /// `AEGIS_BUILD_ID` (drives manifest emission + the bundler) and
    /// `T38_AUTH_BUILD_ID` (the value `option_env!`'d into the client).
    #[serde(default = "default_aegis_envs")]
    pub build_id_env: Vec<String>,
}

fn default_aegis_prefix() -> String {
    "build".into()
}

fn default_aegis_envs() -> Vec<String> {
    vec!["AEGIS_BUILD_ID".into(), "T38_AUTH_BUILD_ID".into()]
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
