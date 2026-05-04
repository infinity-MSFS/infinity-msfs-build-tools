use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "infinity-msfs")]
#[command(version)]
#[command(about = "MSFS WASM build tooling for Infinity Rust projects")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Build everything in `infinity-msfs.toml`: `[[rust.packages]]`
    /// then `[[js.instruments]]`.
    Build(BuildArgs),
    #[command(alias = "list-projects")]
    Projects(ProjectsArgs),
    /// Manage the local MSFS 2024 SDK installation.
    Sdk(SdkArgs),
    /// Run pre-flight checks for the build environment.
    Doctor,
    /// Compile MSFS project XML files via `fspackagetool.exe`.
    /// Windows-only and requires MSFS 2024 to be installed locally,
    /// since the package tool drives a partial sim instance to do
    /// the actual asset compilation.
    Package(PackageArgs),
    /// Watch source files and rebuild on change.
    Watch(WatchArgs),
    /// Scaffold a new MSFS project from a built-in template.
    Create(CreateArgs),
}

#[derive(Debug, Args, Clone)]
pub struct CreateArgs {
    /// Target directory. Prompts when omitted.
    pub path: Option<std::path::PathBuf>,

    /// Skip the picker and use a known template id
    /// (e.g. `rust-wasm-gauge`, `ts-react`, `rescript-react`).
    #[arg(short = 't', long = "template")]
    pub template: Option<String>,

    /// Skip the project_name prompt.
    #[arg(short = 'n', long = "name")]
    pub name: Option<String>,

    /// Fail if any required field would prompt. For CI / scripted use.
    #[arg(long = "no-input")]
    pub no_input: bool,

    /// Allow scaffolding into a non-empty directory.
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Args, Clone)]
pub struct BuildArgs {
    #[arg(long)]
    pub release: bool,

    /// Stream subprocess output directly instead of the compact progress UI.
    #[arg(short = 'v', long)]
    pub verbose: bool,

    /// Restrict the build to the named entries. Matched against
    /// `[[rust.packages]].cargo_package` AND `[[js.instruments]].name`.
    /// May be passed multiple times.
    #[arg(long = "only")]
    pub only: Vec<String>,

    /// Skip the JS pipeline.
    #[arg(long = "rust-only", conflicts_with = "js_only")]
    pub rust_only: bool,

    /// Skip the cargo pipeline.
    #[arg(long = "js-only")]
    pub js_only: bool,

    #[arg(long = "no-wasm-opt")]
    pub no_wasm_opt: bool,

    /// Minify bundled JS.
    #[arg(long)]
    pub minify: bool,

    /// Emit JS sourcemaps. One of `inline`, `external`, `file`.
    #[arg(long = "sourcemap")]
    pub sourcemap: Option<String>,

    /// Skip simulator-package emission for JS instruments
    /// (produce the raw rolldown bundle only).
    #[arg(long = "skip-simulator-package")]
    pub skip_simulator_package: bool,
}

#[derive(Debug, Args, Clone)]
pub struct SdkArgs {
    #[command(subcommand)]
    pub command: SdkCommand,
}

#[derive(Debug, Subcommand, Clone)]
pub enum SdkCommand {
    /// Download the latest MSFS 2024 SDK from sdk.flightsimulator.com and
    /// extract the relevant subtree into the local cache.
    Install(SdkInstallArgs),
    /// Print the resolved SDK path.
    Path,
    /// Remove the cached SDK installation.
    Remove,
}

#[derive(Debug, Args, Clone)]
pub struct SdkInstallArgs {
    /// Re-download even if the latest version is already cached.
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Args, Clone)]
pub struct ProjectsArgs {
    /// Restrict the list to the named entries. Matched against
    /// `[[rust.packages]].cargo_package` AND `[[js.instruments]].name`.
    /// May be passed multiple times.
    #[arg(long = "only")]
    pub only: Vec<String>,
}

#[derive(Debug, Args, Clone)]
pub struct PackageArgs {
    /// Restrict to the named `[[sim_packages]]` entries by `name`.
    /// May be passed multiple times.
    #[arg(long = "only")]
    pub only: Vec<String>,

    /// Stream subprocess output directly instead of the compact UI.
    #[arg(short = 'v', long)]
    pub verbose: bool,

    /// Override `rebuild` for every selected entry.
    #[arg(long)]
    pub rebuild: bool,

    /// Override `mirror` for every selected entry.
    #[arg(long)]
    pub mirror: bool,

    /// Override `force_steam` for every selected entry.
    #[arg(long = "force-steam")]
    pub force_steam: bool,

    /// Export to Marketplace into this directory (overrides per-entry
    /// `marketplace`). Passed through to fspackagetool's
    /// `-marketplace`.
    #[arg(long)]
    pub marketplace: Option<String>,
}

#[derive(Debug, Args, Clone)]
pub struct WatchArgs {
    #[arg(long)]
    pub release: bool,

    /// Stream subprocess output directly instead of the compact UI.
    #[arg(short = 'v', long)]
    pub verbose: bool,

    /// Restrict to the named entries. Matched against
    /// `[[rust.packages]].cargo_package` AND `[[js.instruments]].name`.
    /// May be passed multiple times.
    #[arg(long = "only")]
    pub only: Vec<String>,

    #[arg(long = "no-wasm-opt")]
    pub no_wasm_opt: bool,

    /// Skip the JS pipeline.
    #[arg(long = "rust-only", conflicts_with = "js_only")]
    pub rust_only: bool,

    /// Skip the cargo pipeline.
    #[arg(long = "js-only")]
    pub js_only: bool,

    /// Debounce window in milliseconds. Events that arrive within
    /// this window after the last event collapse into one rebuild.
    #[arg(long, default_value_t = 300)]
    pub debounce: u64,
}
