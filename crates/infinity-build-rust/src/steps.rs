use crate::{
    config::{ArtifactKind, CopyRule},
    plan::BuildPlan,
};
use anyhow::{Context, Result, bail};
use infinity_build_core::{BuildResult, Runner};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

pub fn built_artifact_path(root: &Path, plan: &BuildPlan, release: bool) -> PathBuf {
    let profile = if release { "release" } else { "debug" };
    let mut path = root.join("target");
    if let Some(target) = &plan.target {
        path.push(target);
    }
    path.push(profile);

    let file = match plan.kind {
        // Cargo normalizes wasm bin stems to underscores; native exes keep hyphens.
        ArtifactKind::Wasm => format!("{}.wasm", plan.bin.replace('-', "_")),
        ArtifactKind::Native => {
            if cfg!(target_os = "windows") {
                format!("{}.exe", plan.bin)
            } else {
                plan.bin.clone()
            }
        }
    };
    path.push(file);
    path
}

pub fn run_cargo_build(
    runner: &dyn Runner,
    root: &Path,
    plan: &BuildPlan,
    release: bool,
) -> BuildResult<()> {
    let mut cmd = Command::new("cargo");
    cmd.current_dir(root)
        .arg("build")
        .arg("-p")
        .arg(&plan.package);

    if let Some(target) = &plan.target {
        cmd.arg("--target").arg(target);
    }
    if !plan.features.is_empty() {
        cmd.arg("--features").arg(plan.features.join(","));
    }
    if release {
        cmd.arg("--release");
    }

    // WASM builds that pull in C dependencies (e.g. the bundled SQLite in
    // `t38-navdata`, compiled by the `cc` crate via `libsqlite3-sys`) need a
    // WASI sysroot for the C headers. Point `cc` at the MSFS SDK's bundled
    // `wasi-sysroot` so these builds are self-contained — no per-developer
    // environment setup. An explicit `WASI_SYSROOT` in the environment wins.
    if plan.kind == ArtifactKind::Wasm && std::env::var_os("WASI_SYSROOT").is_none() {
        if let Ok(sdk) = infinity_build_sdk::sdk_path() {
            let sysroot = Path::new(&sdk).join("WASM").join("wasi-sysroot");
            if sysroot.is_dir() {
                cmd.env("WASI_SYSROOT", sysroot);
            }
        }
    }

    runner.run(&mut cmd, "cargo build")
}

pub fn run_wasm_opt(
    runner: &dyn Runner,
    root: &Path,
    passes: &[String],
    input: &Path,
    output: &Path,
) -> BuildResult<()> {
    let mut cmd = Command::new("wasm-opt");
    cmd.current_dir(root);
    for pass in passes {
        cmd.arg(pass);
    }
    cmd.arg("-o").arg(output).arg(input);
    runner.run(&mut cmd, "wasm-opt")
}

pub fn run_copy_rules(root: &Path, rules: &[CopyRule]) -> Result<usize> {
    for rule in rules {
        let from = root.join(&rule.from);
        let to = root.join(&rule.to);
        if !from.exists() {
            bail!(
                "copy source does not exist: {} (configured destination: {})",
                from.display(),
                to.display()
            );
        }
        copy_file(&from, &to)?;
    }
    Ok(rules.len())
}

/// On Windows native + simconnect builds, drop SimConnect.dll next to the
/// .exe so the output is runnable without manual setup.
pub fn copy_simconnect_runtime(plan: &BuildPlan) -> Result<usize> {
    if plan.kind != ArtifactKind::Native
        || !cfg!(target_os = "windows")
        || !plan.features.iter().any(|f| f == "simconnect")
    {
        return Ok(0);
    }

    let sdk = match infinity_build_sdk::sdk_path() {
        Ok(p) => PathBuf::from(p),
        Err(_) => return Ok(0),
    };

    let dll = sdk.join("SimConnect SDK").join("lib").join("SimConnect.dll");
    if !dll.exists() {
        return Ok(0);
    }

    copy_file(&dll, &plan.output_dir.join("SimConnect.dll"))?;
    Ok(1)
}

fn copy_file(from: &Path, to: &Path) -> Result<u64> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory {}", parent.display()))?;
    }
    fs::copy(from, to).with_context(|| {
        format!(
            "failed to copy file from {} to {}",
            from.display(),
            to.display()
        )
    })
}
