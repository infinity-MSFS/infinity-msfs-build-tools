use crate::{
    cargo_meta,
    cli::{BuildArgs, ProjectsArgs},
    config::{BuildConfig, CopyRule, InfinityMsfsToml, PackageBuild, PackageKind},
    process, scripts, sdk_install, stats,
    ui::{self, BuildOutcome, BuildPhase, BuildUi},
    util,
};
use anyhow::{Context, Result, bail};
use cargo_metadata::Metadata;
use console::style;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

/// Resolved, ready-to-execute build description for a single artefact.
struct BuildPlan {
    package: String,
    bin: String,
    target: Option<String>,
    out_dir: PathBuf,
    out_name: String,
    kind: PackageKind,
    features: Vec<String>,
    copy: Vec<CopyRule>,
}

impl BuildPlan {
    fn target_label(&self) -> String {
        self.target.clone().unwrap_or_else(|| "<host>".to_string())
    }
}

pub fn run_build(args: BuildArgs) -> Result<()> {
    sdk_install::ensure_sdk()?;

    let root = util::find_project_root()?;
    let config_path = util::config_path(&root);

    let cfg = if config_path.exists() {
        InfinityMsfsToml::load(&config_path)?
    } else {
        InfinityMsfsToml::default()
    };

    let metadata = cargo_meta::load_metadata(&root)?;

    let mut plans = resolve_plans(&root, &cfg, &metadata, args.package.as_deref(), &args.only)?;

    // Drop native plans on non-Windows hosts with a visible warning rather
    // than failing the whole build. SimConnect's import library is only
    // shipped for Windows, so trying to link there is doomed.
    if !cfg!(target_os = "windows") {
        plans.retain(|plan| {
            if plan.kind == PackageKind::Native {
                eprintln!(
                    "{} skipping native package {} (only built on Windows)",
                    style("!").yellow().bold(),
                    style(&plan.package).bold(),
                );
                false
            } else {
                true
            }
        });
    }

    if plans.is_empty() {
        bail!("no packages selected to build");
    }

    let use_wasm_opt = cfg.wasm_opt.enabled && !args.no_wasm_opt;
    let mut ui = BuildUi::new(&root, plans.len(), args.release, use_wasm_opt, args.verbose);

    ui.announce_phase("Running pre-build scripts", cfg.scripts.pre_build.len());
    scripts::run_script_list(&root, "pre_build", &cfg.scripts.pre_build, args.verbose)?;

    let mut stats_db = stats::Stats::load(&root);

    for plan in &plans {
        ui.start_package(&plan.package);
        let outcome = build_one(
            &root,
            plan,
            &cfg.wasm_opt.args,
            use_wasm_opt,
            args.release,
            args.verbose,
            &mut ui,
            &mut stats_db,
        )?;
        ui.finish_package(&plan.package, &plan.out_dir.join(&plan.out_name), outcome);
    }

    if let Err(err) = stats_db.save(&root) {
        eprintln!(
            "{} failed to persist build stats: {err:#}",
            console::style("warning:").yellow().bold()
        );
    }

    ui.announce_phase("Running post-build scripts", cfg.scripts.post_build.len());
    scripts::run_script_list(&root, "post_build", &cfg.scripts.post_build, args.verbose)?;

    ui.finish();
    Ok(())
}

pub fn run_projects(args: ProjectsArgs) -> Result<()> {
    let root = util::find_project_root()?;
    let config_path = util::config_path(&root);

    let cfg = if config_path.exists() {
        InfinityMsfsToml::load(&config_path)?
    } else {
        InfinityMsfsToml::default()
    };

    let metadata = cargo_meta::load_metadata(&root)?;
    let plans = resolve_plans(&root, &cfg, &metadata, args.package.as_deref(), &args.only)?;

    if plans.is_empty() {
        bail!("no packages selected to list");
    }

    ui::print_projects(
        root.as_path(),
        plans.into_iter().map(|plan| {
            let target = plan.target_label();
            (
                plan.package,
                plan.bin,
                target,
                plan.out_dir.join(plan.out_name),
            )
        }),
    );

    Ok(())
}

fn build_one(
    root: &Path,
    plan: &BuildPlan,
    wasm_opt_args: &[String],
    use_wasm_opt: bool,
    release: bool,
    verbose: bool,
    ui: &mut BuildUi,
    stats_db: &mut stats::Stats,
) -> Result<BuildOutcome> {
    let built = built_artifact_path(root, plan, release);
    let final_path = plan.out_dir.join(&plan.out_name);

    ui.set_phase(&plan.package, BuildPhase::Compiling);
    run_cargo_build(root, plan, release, verbose)?;

    if !built.exists() {
        bail!(
            "cargo build completed, but built artifact was not found at {}",
            built.display()
        );
    }

    fs::create_dir_all(&plan.out_dir).with_context(|| {
        format!(
            "failed to create output directory {}",
            plan.out_dir.display()
        )
    })?;

    let run_opt = use_wasm_opt && plan.kind == PackageKind::Wasm;
    if run_opt {
        ui.set_phase(&plan.package, BuildPhase::Optimizing);
        run_wasm_opt(root, wasm_opt_args, &built, &final_path, verbose)?;
    } else {
        ui.set_phase(&plan.package, BuildPhase::Copying);
        util::copy_file(&built, &final_path)?;
    }

    ui.set_phase(&plan.package, BuildPhase::Copying);
    let mut copied_files = run_copy_rules(root, &plan.copy)?;
    copied_files += copy_simconnect_runtime(plan)?;

    let size_bytes = fs::metadata(&final_path).ok().map(|m| m.len());
    let previous_size_bytes = stats_db.previous_size(&plan.package);
    if let Some(size) = size_bytes {
        stats_db.record(&plan.package, size);
    }

    Ok(BuildOutcome {
        copied_files,
        size_bytes,
        previous_size_bytes,
    })
}

/// For Windows native packages that opt into the `simconnect` feature,
/// copy `SimConnect.dll` from the resolved SDK location next to the built
/// executable so the result is runnable without manual setup.
fn copy_simconnect_runtime(plan: &BuildPlan) -> Result<usize> {
    if plan.kind != PackageKind::Native
        || !cfg!(target_os = "windows")
        || !plan.features.iter().any(|f| f == "simconnect")
    {
        return Ok(0);
    }

    let sdk = match crate::sdk::sdk_path() {
        Ok(p) => PathBuf::from(p),
        Err(_) => return Ok(0),
    };

    let dll = sdk
        .join("SimConnect SDK")
        .join("lib")
        .join("SimConnect.dll");
    if !dll.exists() {
        return Ok(0);
    }

    let dest = plan.out_dir.join("SimConnect.dll");
    util::copy_file(&dll, &dest)?;
    Ok(1)
}

/// Build a list of `BuildPlan`s honouring `[[packages]]` when present and
/// falling back to the legacy single-`[build]` path otherwise. Applies any
/// CLI filters (`-p` / `--only`) before returning.
fn resolve_plans(
    root: &Path,
    cfg: &InfinityMsfsToml,
    metadata: &Metadata,
    cli_package: Option<&str>,
    only: &[String],
) -> Result<Vec<BuildPlan>> {
    let mut plans: Vec<BuildPlan> = if !cfg.packages.is_empty() {
        cfg.packages
            .iter()
            .map(|entry| plan_from_package_entry(root, metadata, &cfg.build, entry))
            .collect::<Result<Vec<_>>>()?
    } else {
        vec![plan_from_legacy(root, metadata, &cfg.build, cli_package)?]
    };

    let filters = collect_filters(cli_package, only);
    if !filters.is_empty() {
        let before = plans.len();
        plans.retain(|p| filters.iter().any(|f| f == &p.package));
        if plans.is_empty() {
            bail!(
                "no configured package matched filter {:?} (had {} candidate{})",
                filters,
                before,
                if before == 1 { "" } else { "s" }
            );
        }
    }

    Ok(plans)
}

fn collect_filters(cli_package: Option<&str>, only: &[String]) -> Vec<String> {
    let mut out: Vec<String> = only.to_vec();
    if let Some(p) = cli_package {
        if !out.iter().any(|candidate| candidate == p) {
            out.push(p.to_string());
        }
    }
    out
}

fn plan_from_package_entry(
    root: &Path,
    metadata: &Metadata,
    base: &BuildConfig,
    entry: &PackageBuild,
) -> Result<BuildPlan> {
    let pkg = cargo_meta::resolve_package(metadata, Some(&entry.package))?;
    let bin = cargo_meta::resolve_bin_name(pkg, entry.bin.as_deref().or(base.bin.as_deref()));

    let kind = entry.kind.unwrap_or(base.kind);

    // For native packages we default the cargo target to the host triple
    // (i.e. don't pass --target at all). For wasm packages we keep the
    // configured target string.
    let target = match (entry.target.as_deref(), kind) {
        (Some(t), _) => Some(t.to_string()),
        (None, PackageKind::Wasm) => Some(base.target.clone()),
        (None, PackageKind::Native) => None,
    };

    let out_dir_rel = entry
        .out_dir
        .clone()
        .unwrap_or_else(|| base.out_dir.clone());
    let out_dir = root.join(&out_dir_rel);

    let out_name = entry
        .out_name
        .clone()
        .or_else(|| {
            // Inherit top-level out_name only when the top-level build.package
            // matches this entry — otherwise we would overwrite the same file
            // for every package in the list.
            match &base.package {
                Some(bp) if bp == &entry.package => base.out_name.clone(),
                _ => None,
            }
        })
        .unwrap_or_else(|| default_out_name(&bin, kind));

    let mut copy = base.copy.clone();
    copy.extend(entry.copy.iter().cloned());

    let mut features = base.features.clone();
    for feat in &entry.features {
        if !features.contains(feat) {
            features.push(feat.clone());
        }
    }

    Ok(BuildPlan {
        package: pkg.name.clone(),
        bin,
        target,
        out_dir,
        out_name,
        kind,
        features,
        copy,
    })
}

fn plan_from_legacy(
    root: &Path,
    metadata: &Metadata,
    base: &BuildConfig,
    cli_package: Option<&str>,
) -> Result<BuildPlan> {
    let package_name = cli_package
        .map(|s| s.to_string())
        .or_else(|| base.package.clone());

    let pkg = cargo_meta::resolve_package(metadata, package_name.as_deref())?;
    let bin = cargo_meta::resolve_bin_name(pkg, base.bin.as_deref());

    let kind = base.kind;
    let target = match kind {
        PackageKind::Wasm => Some(base.target.clone()),
        PackageKind::Native => None,
    };
    let out_dir = root.join(&base.out_dir);
    let out_name = base
        .out_name
        .clone()
        .unwrap_or_else(|| default_out_name(&bin, kind));

    Ok(BuildPlan {
        package: pkg.name.clone(),
        bin,
        target,
        out_dir,
        out_name,
        kind,
        features: base.features.clone(),
        copy: base.copy.clone(),
    })
}

fn default_out_name(bin: &str, kind: PackageKind) -> String {
    match kind {
        PackageKind::Wasm => format!("{bin}.wasm"),
        PackageKind::Native => {
            if cfg!(target_os = "windows") {
                format!("{bin}.exe")
            } else {
                bin.to_string()
            }
        }
    }
}

fn built_artifact_path(root: &Path, plan: &BuildPlan, release: bool) -> PathBuf {
    let profile = if release { "release" } else { "debug" };

    let mut path = root.join("target");
    if let Some(target) = &plan.target {
        path.push(target);
    }
    path.push(profile);

    let file = match plan.kind {
        // For wasm bin targets cargo normalizes the file stem to use
        // underscores; for native exes it keeps hyphens.
        PackageKind::Wasm => format!("{}.wasm", plan.bin.replace('-', "_")),
        PackageKind::Native => {
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

fn run_cargo_build(root: &Path, plan: &BuildPlan, release: bool, verbose: bool) -> Result<()> {
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

    process::run_command(&mut cmd, "cargo build", verbose)?;
    Ok(())
}

fn run_wasm_opt(
    root: &Path,
    opt_args: &[String],
    input: &Path,
    output: &Path,
    verbose: bool,
) -> Result<()> {
    let mut cmd = Command::new("wasm-opt");
    cmd.current_dir(root);

    for arg in opt_args {
        cmd.arg(arg);
    }

    cmd.arg("-o").arg(output).arg(input);

    process::run_command(&mut cmd, "wasm-opt", verbose)?;
    Ok(())
}

fn run_copy_rules(root: &Path, rules: &[CopyRule]) -> Result<usize> {
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

        util::copy_file(&from, &to)?;
    }
    Ok(rules.len())
}
