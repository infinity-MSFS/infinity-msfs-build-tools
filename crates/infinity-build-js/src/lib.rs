pub mod bundler;
pub mod config;
mod package;
mod templates;

pub use bundler::JsBundler;
pub use config::{Instrument, JsBuildConfig, ModuleAlias, SimulatorPackage, SimulatorPackageKind};

use infinity_build_core::{BuildResult, Builder, SimpleArtifact};
use std::path::Path;

/// Build every instrument in `config`, applying an optional name
/// filter (regex). This is the "I just want to build everything"
/// helper; if you need finer control (per-instrument progress,
/// per-instrument failure handling), construct [`JsBundler`] yourself
/// and drive each [`Instrument`] through it.
///
/// Returns one [`SimpleArtifact`] per *successfully built* instrument
/// in the order they appear in the config.
pub fn build_all(
    config: &JsBuildConfig,
    project_root: &Path,
    filter: Option<&regex::Regex>,
    options: &bundler::BundleOptions,
) -> BuildResult<Vec<SimpleArtifact>> {
    let bundler = JsBundler::new(project_root, options.clone());
    let mut out = Vec::new();
    for instrument in &config.instruments {
        if let Some(re) = filter {
            if !re.is_match(&instrument.name) {
                continue;
            }
        }
        let input = bundler::JsBuildInput {
            instrument: instrument.clone(),
            package: config.package.clone(),
        };
        let artifact = bundler.build(&input)?;
        out.push(artifact.into());
    }
    Ok(out)
}
