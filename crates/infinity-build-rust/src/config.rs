use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct RustConfig {
    #[serde(default = "default_target")]
    pub default_target: String,

    #[serde(default = "default_output_dir")]
    pub output_dir: String,

    #[serde(default)]
    pub copy_files: Vec<CopyRule>,

    #[serde(default)]
    pub wasm_opt: WasmOptConfig,

    #[serde(default)]
    pub packages: Vec<RustPackage>,
}

impl Default for RustConfig {
    fn default() -> Self {
        Self {
            default_target: default_target(),
            output_dir: default_output_dir(),
            copy_files: Vec::new(),
            wasm_opt: WasmOptConfig::default(),
            packages: Vec::new(),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct RustPackage {
    pub cargo_package: String,

    pub cargo_bin: Option<String>,

    /// Override `[rust].default_target`. Ignored for `native` artifacts.
    pub target: Option<String>,

    pub output_dir: Option<String>,

    pub artifact_name: Option<String>,

    pub artifact_kind: Option<ArtifactKind>,

    #[serde(default)]
    pub cargo_features: Vec<String>,

    #[serde(default)]
    pub copy_files: Vec<CopyRule>,
}

/// `wasm` artifacts target `wasm32-wasip1` (or whatever `target` is set to)
/// and run through wasm-opt. `native` artifacts target the host triple,
/// only build on Windows (SimConnect's import lib is Windows-only), and
/// skip wasm-opt.
#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ArtifactKind {
    #[default]
    Wasm,
    Native,
}

#[derive(Debug, Deserialize, Clone)]
pub struct WasmOptConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,

    #[serde(default = "default_passes")]
    pub passes: Vec<String>,
}

impl Default for WasmOptConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            passes: default_passes(),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct CopyRule {
    pub from: String,
    pub to: String,
}

fn default_target() -> String {
    "wasm32-wasip1".to_string()
}

fn default_output_dir() -> String {
    "build/msfs".to_string()
}

fn default_true() -> bool {
    true
}

fn default_passes() -> Vec<String> {
    vec![
        "-O1".to_string(),
        "--signext-lowering".to_string(),
        "--enable-bulk-memory".to_string(),
        "--enable-nontrapping-float-to-int".to_string(),
    ]
}
