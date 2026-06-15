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
    /// Directory holding `msfs_host_shim.dll.lib`, for `native-dynamic` builds.
    pub shim_lib_dir: Option<PathBuf>,
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
    force_native_dynamic: bool,
) -> Result<Vec<BuildPlan>> {
    if rust.packages.is_empty() {
        bail!(
            "no [[rust.packages]] entries configured. Add at least one to build."
        );
    }

    let mut plans: Vec<BuildPlan> = rust
        .packages
        .iter()
        .map(|entry| plan_from_entry(root, metadata, rust, entry, force_native_dynamic))
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
    force_native_dynamic: bool,
) -> Result<BuildPlan> {
    let pkg = cargo_meta::resolve_package(metadata, Some(&entry.cargo_package))?;
    let bin = cargo_meta::resolve_bin_name(pkg, entry.cargo_bin.as_deref());

    // `--native` rebuilds every wasm gauge as an emulator `.dll`. SimConnect
    // exe (`native`) entries are left alone — they aren't gauges.
    let mut kind = entry.artifact_kind.unwrap_or_default();
    if force_native_dynamic && kind == ArtifactKind::Wasm {
        kind = ArtifactKind::NativeDynamic;
    }

    let target = match (entry.target.as_deref(), kind) {
        // An explicit `target` is honoured for wasm; a native/native-dynamic
        // host build ignores it (always the host triple).
        (Some(t), ArtifactKind::Wasm) => Some(t.to_string()),
        (_, ArtifactKind::Wasm) => Some(rust.default_target.clone()),
        (_, ArtifactKind::Native | ArtifactKind::NativeDynamic) => None,
    };

    let output_dir_rel = entry
        .output_dir
        .clone()
        .unwrap_or_else(|| rust.output_dir.clone());
    let output_dir = root.join(&output_dir_rel);

    let artifact_name = entry
        .artifact_name
        .clone()
        // A forced native-dynamic build ignores a wasm `artifact_name` override
        // (it'd carry a `.wasm` suffix); derive the `.dll` name instead.
        .filter(|_| !(force_native_dynamic && kind == ArtifactKind::NativeDynamic))
        .unwrap_or_else(|| default_artifact_name(&bin, kind));

    // The import library is only linked on Windows; Linux/macOS bind the
    // gauge's host imports to the shim at load time, so no shim dir is needed.
    let shim_lib_dir = if kind == ArtifactKind::NativeDynamic && cfg!(target_os = "windows") {
        Some(resolve_shim_lib_dir(root, rust)?)
    } else {
        None
    };

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
        shim_lib_dir,
    })
}

/// Locate the emulator shim's import-library directory for `native-dynamic`
/// builds: `[rust].shim_lib_dir` (relative to root) or `INFINITY_EMU_SHIM_DIR`.
fn resolve_shim_lib_dir(root: &Path, rust: &RustConfig) -> Result<PathBuf> {
    let raw = rust
        .shim_lib_dir
        .clone()
        .or_else(|| std::env::var("INFINITY_EMU_SHIM_DIR").ok())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "native-dynamic build needs the emulator shim import library. Set \
                 `[rust].shim_lib_dir` in infinity-msfs.toml or the INFINITY_EMU_SHIM_DIR \
                 env var to the directory containing msfs_host_shim.dll.lib."
            )
        })?;
    let dir = PathBuf::from(&raw);
    let dir = if dir.is_absolute() { dir } else { root.join(dir) };
    if !dir.join("msfs_host_shim.dll.lib").exists() {
        bail!(
            "msfs_host_shim.dll.lib not found in {} — build the shim first \
             (`cargo build -p msfs-host-shim` in infinity-emu).",
            dir.display()
        );
    }
    Ok(dir)
}

pub fn default_artifact_name(bin: &str, kind: ArtifactKind) -> String {
    match kind {
        ArtifactKind::Wasm => format!("{bin}.wasm"),
        // Platform cdylib name (`foo.dll` / `libfoo.so` / `libfoo.dylib`). `bin`
        // is already underscored, so this is `{bin}.dll` on Windows as before.
        ArtifactKind::NativeDynamic => crate::steps::cdylib_filename(&bin.replace('-', "_")),
        ArtifactKind::Native => {
            if cfg!(target_os = "windows") {
                format!("{bin}.exe")
            } else {
                bin.to_string()
            }
        }
    }
}
