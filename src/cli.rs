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
    Build(BuildArgs),
    /// Bundle JS/TS instruments using rolldown and emit MSFS
    /// package sources. Requires a `[js]` section in `infinity-msfs.toml`.
    Js(JsArgs),
    #[command(alias = "list-projects")]
    Projects(ProjectsArgs),
    /// Manage the local MSFS 2024 SDK installation.
    Sdk(SdkArgs),
    /// Run pre-flight checks for the build environment.
    Doctor,
}

#[derive(Debug, Args, Clone)]
pub struct BuildArgs {
    #[arg(long)]
    pub release: bool,

    /// Stream subprocess output directly instead of the compact progress UI.
    #[arg(short = 'v', long)]
    pub verbose: bool,

    /// Override the single legacy `[build].package`. Ignored when
    /// `[[packages]]` is set and `--only` is preferred there.
    #[arg(short = 'p', long = "package")]
    pub package: Option<String>,

    /// In multi-package mode, restrict the build to the named packages.
    /// May be passed multiple times. Matched against the `package` field
    /// of each `[[packages]]` entry.
    #[arg(long = "only")]
    pub only: Vec<String>,

    #[arg(long = "no-wasm-opt")]
    pub no_wasm_opt: bool,
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
    /// Override the single legacy `[build].package`. Ignored when
    /// `[[packages]]` is set and `--only` is preferred there.
    #[arg(short = 'p', long = "package")]
    pub package: Option<String>,

    /// In multi-package mode, restrict the list to the named packages.
    /// May be passed multiple times. Matched against the `package` field
    /// of each `[[packages]]` entry.
    #[arg(long = "only")]
    pub only: Vec<String>,
}

#[derive(Debug, Args, Clone)]
pub struct JsArgs {
    /// Stream subprocess output directly instead of the compact UI.
    #[arg(short = 'v', long)]
    pub verbose: bool,

    /// Restrict to instruments whose `name` matches this regex.
    #[arg(short = 'f', long = "filter")]
    pub filter: Option<String>,

    /// Minify the bundled JS.
    #[arg(short = 'm', long)]
    pub minify: bool,

    /// Skip simulator-package emission (just produce the raw bundle).
    #[arg(short = 's', long = "skip-simulator-package")]
    pub skip_simulator_package: bool,

    /// Emit sourcemaps. One of `inline`, `external`, `file`.
    #[arg(short = 'p', long = "sourcemap")]
    pub sourcemap: Option<String>,
}
