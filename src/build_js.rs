use crate::{config::InfinityMsfsToml, ui::{BuildOutcome, BuildPhase, BuildUi}, util};
use anyhow::{Result, bail};
use infinity_build_core::{Artifact, Builder};
use infinity_build_js::{
    JsBundler,
    bundler::{BundleOptions, JsBuildInput, SourceMapKind},
};
use std::{collections::HashMap, path::Path};

pub struct JsBuildOpts<'a> {
    pub verbose: bool,
    pub minify: bool,
    pub skip_simulator_package: bool,
    pub sourcemap: Option<&'a str>,
    /// Empty = build every instrument.
    pub only: &'a [String],
}

pub fn run_js_pipeline(root: &Path, cfg: &InfinityMsfsToml, opts: &JsBuildOpts<'_>) -> Result<()> {
    let Some(js_cfg) = cfg.js.as_ref() else {
        return Ok(());
    };

    let instruments: Vec<_> = js_cfg
        .instruments
        .iter()
        .filter(|i| opts.only.is_empty() || opts.only.iter().any(|n| n == &i.name))
        .collect();

    if instruments.is_empty() {
        return Ok(());
    }

    let bundle_options = BundleOptions {
        bundles_dir: None,
        minify: opts.minify,
        sourcemap: parse_sourcemap_flag(opts.sourcemap)?,
        skip_simulator_package: opts.skip_simulator_package,
        env: env_from_process(),
    };

    let bundler = JsBundler::new(root.to_path_buf(), bundle_options);
    let mut ui = BuildUi::new(root, instruments.len(), false, false, opts.verbose);
    ui.announce_phase("Bundling JS instruments", instruments.len());

    for instrument in &instruments {
        ui.start_package(&instrument.name);
        ui.set_phase(&instrument.name, BuildPhase::Compiling);

        let input = JsBuildInput {
            instrument: (*instrument).clone(),
            package: js_cfg.package.clone(),
        };
        let artifact = bundler.build(&input)?;

        ui.set_phase(&instrument.name, BuildPhase::Copying);

        let primary = artifact
            .primary()
            .map(|f| f.path.clone())
            .unwrap_or_else(|| artifact.bundle_dir.clone());
        let outcome = BuildOutcome {
            copied_files: artifact.files().len(),
            size_bytes: Some(artifact.total_bytes()),
            previous_size_bytes: None,
        };
        ui.finish_package(&instrument.name, &primary, outcome);
    }

    ui.finish();
    let _ = util::config_path; // silence unused if cfg path not used elsewhere
    Ok(())
}

fn parse_sourcemap_flag(flag: Option<&str>) -> Result<Option<SourceMapKind>> {
    match flag {
        None => Ok(None),
        Some("inline") => Ok(Some(SourceMapKind::Inline)),
        Some("external") | Some("linked") => Ok(Some(SourceMapKind::External)),
        Some("file") => Ok(Some(SourceMapKind::File)),
        Some(other) => bail!(
            "unknown --sourcemap value `{}` (expected `inline`, `external`, or `file`)",
            other
        ),
    }
}

/// Filtered to ASCII-identifier-safe keys; rolldown's `define` rejects others.
fn env_from_process() -> HashMap<String, String> {
    std::env::vars()
        .filter(|(k, _)| is_valid_env_identifier(k))
        .collect()
}

fn is_valid_env_identifier(k: &str) -> bool {
    let mut chars = k.chars();
    match chars.next() {
        None => false,
        Some(c) if c.is_ascii_digit() => false,
        Some(c) if !(c.is_ascii_alphabetic() || c == '_') => false,
        _ => chars.all(|c| c.is_ascii_alphanumeric() || c == '_'),
    }
}
