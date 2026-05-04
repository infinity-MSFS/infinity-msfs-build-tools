use crate::{
    cli::JsArgs,
    config::InfinityMsfsToml,
    ui::{BuildOutcome, BuildPhase, BuildUi},
    util,
};
use anyhow::{Context, Result, bail};
use infinity_build_core::{Artifact, Builder};
use infinity_build_js::{
    JsBundler,
    bundler::{BundleOptions, JsBuildInput, SourceMapKind},
};
use std::collections::HashMap;

pub fn run_js(args: JsArgs) -> Result<()> {
    let root = util::find_project_root()?;
    let cfg_path = util::config_path(&root);
    if !cfg_path.exists() {
        bail!(
            "no infinity-msfs.toml found at {}; run `infinity-msfs js` from a project root",
            cfg_path.display()
        );
    }

    let cfg = InfinityMsfsToml::load(&cfg_path)?;
    let js_cfg = cfg.js.as_ref().ok_or_else(|| {
        anyhow::anyhow!(
            "no [js] section in {}; nothing to bundle",
            cfg_path.display()
        )
    })?;

    let filter = match &args.filter {
        Some(s) => {
            Some(regex::Regex::new(s).with_context(|| format!("invalid --filter regex `{s}`"))?)
        }
        None => None,
    };

    let instruments: Vec<_> = js_cfg
        .instruments
        .iter()
        .filter(|i| {
            filter
                .as_ref()
                .map(|re| re.is_match(&i.name))
                .unwrap_or(true)
        })
        .collect();

    if instruments.is_empty() {
        bail!("no JS instruments selected to build");
    }

    let bundle_options = BundleOptions {
        bundles_dir: None,
        minify: args.minify,
        sourcemap: parse_sourcemap_flag(args.sourcemap.as_deref())?,
        skip_simulator_package: args.skip_simulator_package,
        env: env_from_process(),
    };

    let bundler = JsBundler::new(root.clone(), bundle_options);

    // Reuse the same BuildUi the WASM path uses so output looks
    // consistent across both build flavours.
    let mut ui = BuildUi::new(&root, instruments.len(), false, false, args.verbose);
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

/// Forward the host process's environment to rolldown's `define`.
/// Filtered to ASCII-identifier-safe keys so we don't generate
/// `define` entries rolldown would reject.
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
