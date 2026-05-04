//! `fspackagetool.exe` wrapper. Windows-only, requires MSFS 2024
//! installed locally — the package tool drives a partial sim instance
//! to do the actual asset compilation.

use anyhow::{Result, bail};
use infinity_build_core::Runner;
use serde::Deserialize;
use std::{
    path::{Path, PathBuf},
    process::Command,
};

/// One MSFS project file fed to `fspackagetool.exe`.
#[derive(Debug, Deserialize, Clone)]
pub struct SimPackage {
    pub name: String,
    pub project_xml: String,

    #[serde(default)]
    pub output_dir: Option<String>,

    #[serde(default)]
    pub temp_dir: Option<String>,

    #[serde(default)]
    pub marketplace_dir: Option<String>,

    #[serde(default)]
    pub force_rebuild: bool,

    #[serde(default)]
    pub mirror_output: bool,

    #[serde(default)]
    pub prefer_steam: bool,
}

#[derive(Debug, Default, Clone)]
pub struct PackageOverrides {
    pub force_rebuild: bool,
    pub mirror_output: bool,
    pub prefer_steam: bool,
    pub marketplace_dir: Option<String>,
}

pub fn locate_fspackagetool() -> Result<PathBuf> {
    let sdk = infinity_build_sdk::sdk_path().map_err(|e| anyhow::anyhow!(e))?;
    let candidate = PathBuf::from(&sdk)
        .join("Tools")
        .join("bin")
        .join("fspackagetool.exe");
    if !candidate.exists() {
        bail!(
            "fspackagetool.exe not found at {}.\n\
             The cached SDK only contains the WASM/SimConnect subset.\n\
             Install the full MSFS 2024 SDK from sdk.flightsimulator.com\n\
             and set MSFS2024_SDK to its root.",
            candidate.display()
        );
    }
    Ok(candidate)
}

pub fn build_one(
    runner: &dyn Runner,
    root: &Path,
    tool: &Path,
    entry: &SimPackage,
    overrides: &PackageOverrides,
) -> Result<()> {
    let project = root.join(&entry.project_xml);
    if !project.exists() {
        bail!("project XML not found at {}", project.display());
    }

    let mut cmd = Command::new(tool);
    cmd.current_dir(root).arg(&project);

    if let Some(out) = entry.output_dir.as_deref() {
        cmd.arg("-outputdir").arg(root.join(out));
    }
    if let Some(temp) = entry.temp_dir.as_deref() {
        cmd.arg("-tempdir").arg(root.join(temp));
    }

    let marketplace = overrides
        .marketplace_dir
        .as_deref()
        .or(entry.marketplace_dir.as_deref());
    if let Some(mp) = marketplace {
        cmd.arg("-marketplace").arg(root.join(mp));
    }

    if entry.force_rebuild || overrides.force_rebuild {
        cmd.arg("-rebuild");
    }
    if entry.mirror_output || overrides.mirror_output {
        cmd.arg("-mirroring");
    }
    if entry.prefer_steam || overrides.prefer_steam {
        cmd.arg("-forcesteam");
    }

    cmd.arg("-nopause");

    runner
        .run(&mut cmd, &format!("fspackagetool {}", entry.name))
        .map_err(|e| anyhow::anyhow!(e.to_string()))
}
