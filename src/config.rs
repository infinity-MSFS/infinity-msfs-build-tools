use anyhow::{Context, Result};
use infinity_build_js::JsBuildConfig;
use serde::Deserialize;
use std::{fs, path::Path};

#[derive(Debug, Deserialize, Default, Clone)]
pub struct InfinityMsfsToml {
    /// Legacy single-package build config. Still honoured when
    /// `packages` is empty and a package is resolvable (via CLI or
    /// `build.package`).
    #[serde(default)]
    pub build: BuildConfig,

    /// Multi-package build list. When non-empty, each entry is built
    /// in the order given. Fields left unset inherit from [`BuildConfig`].
    #[serde(default)]
    pub packages: Vec<PackageBuild>,

    #[serde(default)]
    pub wasm_opt: WasmOptConfig,

    #[serde(default)]
    pub scripts: ScriptsConfig,

    /// JS/TS instrument bundling config. When present, `build js`
    /// (or `build all`) will bundle each entry via rolldown and
    /// emit MSFS package sources alongside the WASM packages.
    #[serde(default)]
    pub js: Option<JsBuildConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct BuildConfig {
    #[serde(default = "default_target")]
    pub target: String,

    pub package: Option<String>,
    pub bin: Option<String>,

    #[serde(default = "default_out_dir")]
    pub out_dir: String,

    pub out_name: Option<String>,

    #[serde(default)]
    pub kind: PackageKind,

    #[serde(default)]
    pub features: Vec<String>,

    #[serde(default)]
    pub copy: Vec<CopyRule>,
}

impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            target: default_target(),
            package: None,
            bin: None,
            out_dir: default_out_dir(),
            out_name: None,
            kind: PackageKind::default(),
            features: Vec::new(),
            copy: Vec::new(),
        }
    }
}

/// One entry in the `[[packages]]` array. Every field except `package`
/// is optional and inherits from the top-level `[build]` block when
/// omitted.
#[derive(Debug, Deserialize, Clone)]
pub struct PackageBuild {
    pub package: String,
    pub bin: Option<String>,
    pub target: Option<String>,
    pub out_dir: Option<String>,
    pub out_name: Option<String>,
    pub kind: Option<PackageKind>,

    #[serde(default)]
    pub features: Vec<String>,

    #[serde(default)]
    pub copy: Vec<CopyRule>,
}

/// What kind of artefact a package produces.
///
/// `wasm` packages are built for `wasm32-wasip1` (or whatever `target` is
/// configured) and post-processed with `wasm-opt`. `native` packages are
/// built for the host triple, are gated to Windows (since SimConnect's
/// import library is Windows-only), and skip the wasm-opt step.
#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum PackageKind {
    #[default]
    Wasm,
    Native,
}

#[derive(Debug, Deserialize, Clone)]
pub struct WasmOptConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,

    #[serde(default = "default_wasm_opt_args")]
    pub args: Vec<String>,
}

impl Default for WasmOptConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            args: default_wasm_opt_args(),
        }
    }
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct ScriptsConfig {
    #[serde(default)]
    pub pre_build: Vec<String>,

    #[serde(default)]
    pub post_build: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CopyRule {
    pub from: String,
    pub to: String,
}

fn default_target() -> String {
    "wasm32-wasip1".to_string()
}

fn default_out_dir() -> String {
    "build/msfs".to_string()
}

fn default_true() -> bool {
    true
}

fn default_wasm_opt_args() -> Vec<String> {
    vec![
        "-O1".to_string(),
        "--signext-lowering".to_string(),
        "--enable-bulk-memory".to_string(),
        "--enable-nontrapping-float-to-int".to_string(),
    ]
}

impl InfinityMsfsToml {
    pub fn load(path: &Path) -> Result<Self> {
        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read config file {}", path.display()))?;

        let cfg: Self = toml::from_str(&raw)
            .with_context(|| format!("failed to parse TOML in {}", path.display()))?;

        Ok(cfg)
    }
}
