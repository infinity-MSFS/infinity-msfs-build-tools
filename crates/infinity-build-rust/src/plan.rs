use crate::{
    cargo_meta,
    config::{ArtifactKind, CopyRule, RustConfig, RustPackage},
};
use anyhow::{Result, bail};
use cargo_metadata::Metadata;
use std::path::{Path, PathBuf};

pub struct BuildPlan {
    pub package: String,
    pub bin: String,
    pub target: Option<String>,
    pub output_dir: PathBuf,
    pub artifact_name: String,
    pub kind: ArtifactKind,
    pub features: Vec<String>,
    pub copy_files: Vec<CopyRule>,
}

impl BuildPlan {
    pub fn target_label(&self) -> String {
        self.target.clone().unwrap_or_else(|| "<host>".to_string())
    }
}

pub fn resolve_plans(
    root: &Path,
    rust: &RustConfig,
    metadata: &Metadata,
    only: &[String],
) -> Result<Vec<BuildPlan>> {
    if rust.packages.is_empty() {
        bail!(
            "no [[rust.packages]] entries configured. Add at least one to build."
        );
    }

    let mut plans: Vec<BuildPlan> = rust
        .packages
        .iter()
        .map(|entry| plan_from_entry(root, metadata, rust, entry))
        .collect::<Result<Vec<_>>>()?;

    if !only.is_empty() {
        let before = plans.len();
        plans.retain(|p| only.iter().any(|f| f == &p.package));
        if plans.is_empty() {
            bail!(
                "no [[rust.packages]] entry matched filter {:?} (had {} candidate{})",
                only,
                before,
                if before == 1 { "" } else { "s" }
            );
        }
    }

    Ok(plans)
}

fn plan_from_entry(
    root: &Path,
    metadata: &Metadata,
    rust: &RustConfig,
    entry: &RustPackage,
) -> Result<BuildPlan> {
    let pkg = cargo_meta::resolve_package(metadata, Some(&entry.cargo_package))?;
    let bin = cargo_meta::resolve_bin_name(pkg, entry.cargo_bin.as_deref());

    let kind = entry.artifact_kind.unwrap_or_default();

    let target = match (entry.target.as_deref(), kind) {
        (Some(t), _) => Some(t.to_string()),
        (None, ArtifactKind::Wasm) => Some(rust.default_target.clone()),
        (None, ArtifactKind::Native) => None,
    };

    let output_dir_rel = entry
        .output_dir
        .clone()
        .unwrap_or_else(|| rust.output_dir.clone());
    let output_dir = root.join(&output_dir_rel);

    let artifact_name = entry
        .artifact_name
        .clone()
        .unwrap_or_else(|| default_artifact_name(&bin, kind));

    let mut copy_files = rust.copy_files.clone();
    copy_files.extend(entry.copy_files.iter().cloned());

    Ok(BuildPlan {
        package: pkg.name.clone(),
        bin,
        target,
        output_dir,
        artifact_name,
        kind,
        features: entry.cargo_features.clone(),
        copy_files,
    })
}

pub fn default_artifact_name(bin: &str, kind: ArtifactKind) -> String {
    match kind {
        ArtifactKind::Wasm => format!("{bin}.wasm"),
        ArtifactKind::Native => {
            if cfg!(target_os = "windows") {
                format!("{bin}.exe")
            } else {
                bin.to_string()
            }
        }
    }
}
