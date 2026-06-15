use crate::{
    build_js::{self, JsBuildOpts},
    cli::{BuildArgs, ProjectsArgs},
    config::InfinityMsfsToml,
    runner::CliRunner,
    ui::{self, BuildOutcome, BuildPhase, BuildUi},
    util,
};
use anyhow::{Context, Result, bail};
use console::style;
use infinity_build_rust::{
    ArtifactKind, BuildPlan, Stats, built_artifact_path, cargo_meta, copy_simconnect_runtime,
    hooks, plan::resolve_plans, run_cargo_build, run_copy_rules, run_wasm_opt,
};
use infinity_build_sdk as sdk;
use std::{fs, path::Path};

pub fn run_build(args: BuildArgs) -> Result<()> {
    let root = util::find_project_root()?;
    let cfg = load_cfg(&root)?;
    let runner = CliRunner { verbose: args.verbose };

    // Export the aegis build id before anything compiles. cargo (a child of
    // this process) and the pre/post hooks all inherit it, so the value baked
    // into the WASM, the emitted `#[protected]` manifests, and the post-build
    // seal/upload are guaranteed identical.
    apply_aegis_build_id(&root, &cfg, args.release)?;

    let do_rust = !args.js_only;
    // `--native` targets the off-sim emulator, which has no JS bridge — skip it.
    let do_js = !args.rust_only && !args.native && cfg.js.is_some();

    let any_rust = do_rust && !cfg.rust.packages.is_empty();
    let any_js = do_js;

    if !any_rust && !any_js {
        bail!(
            "nothing to build. Add `[[rust.packages]]` or a `[js]` section to {}",
            util::config_path(&root).display()
        );
    }

    // Hooks run on release builds only. The post hook seals the lowered
    // `#[protected]` programs and uploads them to R2 — but debug builds ship the
    // real kernels (not lowered stubs) and gate entitlement-only, so there's
    // nothing to seal or upload. Skipping the hooks keeps debug iteration fast
    // and avoids touching the live R2 store.
    if args.release {
        ui::announce_pre_hook(cfg.hooks.pre.len());
        hooks::run_hook_list(&runner, &root, "pre", &cfg.hooks.pre)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    } else {
        ui::announce_hooks_skipped("pre", cfg.hooks.pre.len());
    }

    if any_rust {
        run_rust_pipeline(&root, &cfg, &args, &runner)?;
    }

    if any_js {
        let js_opts = JsBuildOpts {
            verbose: args.verbose,
            minify: args.minify,
            skip_simulator_package: args.skip_simulator_package,
            sourcemap: args.sourcemap.as_deref(),
            only: &args.only,
        };
        build_js::run_js_pipeline(&root, &cfg, &js_opts)?;
    }

    if args.release {
        ui::announce_post_hook(cfg.hooks.post.len());
        hooks::run_hook_list(&runner, &root, "post", &cfg.hooks.post)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    } else {
        ui::announce_hooks_skipped("post", cfg.hooks.post.len());
    }

    Ok(())
}

fn run_rust_pipeline(
    root: &Path,
    cfg: &InfinityMsfsToml,
    args: &BuildArgs,
    runner: &CliRunner,
) -> Result<()> {
    let metadata = cargo_meta::load_metadata(root)?;
    let mut plans = resolve_plans(root, &cfg.rust, &metadata, &args.only, args.native)?;

    // A standalone SimConnect client (`native`) links SimConnect's Windows-only
    // import library, so it's Windows-only. A `native-dynamic` emulator gauge
    // links nothing platform-specific (it binds to the shim at load), so it
    // builds on every OS — `.dll` on Windows, `.so` on Linux, `.dylib` on macOS.
    if !cfg!(target_os = "windows") {
        plans.retain(|plan| {
            if matches!(plan.kind, ArtifactKind::Native) {
                eprintln!(
                    "{} skipping native package {} (SimConnect exe is Windows-only)",
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
        // Filter eliminated everything (only matched JS instruments).
        // Not an error — JS step still runs.
        return Ok(());
    }

    // The MSFS SDK is only needed for wasm builds (its wasi-sysroot supplies C
    // headers) and native SimConnect exes (its import library). A native-dynamic
    // emulator build links only the shim, so don't force an SDK download for it.
    if plans
        .iter()
        .any(|p| matches!(p.kind, ArtifactKind::Wasm | ArtifactKind::Native))
    {
        sdk::ensure_sdk()?;
    }

    let use_wasm_opt = cfg.rust.wasm_opt.enabled && !args.no_wasm_opt;
    let mut ui = BuildUi::new(root, plans.len(), args.release, use_wasm_opt, args.verbose);

    let mut stats_db = Stats::load(root);

    for plan in &plans {
        ui.start_package(&plan.package);
        let outcome = build_one(
            root,
            plan,
            &cfg.rust.wasm_opt.passes,
            use_wasm_opt,
            args.release,
            runner,
            &mut ui,
            &mut stats_db,
        )?;
        ui.finish_package(
            &plan.package,
            &plan.output_dir.join(&plan.artifact_name),
            outcome,
        );
    }

    if let Err(err) = stats_db.save(root) {
        eprintln!(
            "{} failed to persist build stats: {err:#}",
            style("warning:").yellow().bold()
        );
    }

    ui.finish();
    Ok(())
}

pub fn run_projects(args: ProjectsArgs) -> Result<()> {
    let root = util::find_project_root()?;
    let cfg = load_cfg(&root)?;
    let metadata = cargo_meta::load_metadata(&root)?;

    let plans = resolve_plans(&root, &cfg.rust, &metadata, &args.only, false).unwrap_or_default();

    let js_instruments: Vec<(String, String)> = match &cfg.js {
        Some(js) => js
            .instruments
            .iter()
            .filter(|i| args.only.is_empty() || args.only.iter().any(|n| n == &i.name))
            .map(|i| (i.name.clone(), i.index.to_string_lossy().into_owned()))
            .collect(),
        None => Vec::new(),
    };

    if plans.is_empty() && js_instruments.is_empty() {
        bail!("no projects matched");
    }

    ui::print_projects(
        root.as_path(),
        plans.into_iter().map(|plan| {
            let target = plan.target_label();
            (
                plan.package,
                plan.bin,
                target,
                plan.output_dir.join(plan.artifact_name),
            )
        }),
        js_instruments,
    );

    Ok(())
}

/// Resolve the aegis build id and export it into the configured env vars so
/// the build and its hooks see a single, consistent value. No-op unless
/// `[aegis].enabled`.
fn apply_aegis_build_id(root: &Path, cfg: &InfinityMsfsToml, release: bool) -> Result<()> {
    let a = &cfg.aegis;
    if !a.enabled {
        return Ok(());
    }
    let build_id = match a.build_id.as_deref().filter(|s| !s.is_empty()) {
        Some(explicit) => explicit.to_string(),
        None => derive_git_build_id(root, &a.build_id_prefix)?,
    };
    let profile = if release { "release" } else { "debug" };
    for var in &a.build_id_env {
        // SAFETY: set before any threads are spawned; single-threaded here.
        unsafe { std::env::set_var(var, &build_id) };
    }
    // Lets the post-build seal/upload hook decide whether to push to the live
    // R2 store (release) or just bundle locally (debug iterations).
    unsafe { std::env::set_var("AEGIS_BUILD_PROFILE", profile) };
    eprintln!(
        "{} aegis build id {} [{profile}] (exported to {})",
        style("aegis:").cyan().bold(),
        style(&build_id).bold(),
        a.build_id_env.join(", ")
    );
    Ok(())
}

/// `<prefix>-<git short sha>` with a `-dirty` suffix when the tree has
/// uncommitted changes.
fn derive_git_build_id(root: &Path, prefix: &str) -> Result<String> {
    use std::process::Command;
    let sha_out = Command::new("git")
        .current_dir(root)
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .context("running `git rev-parse` to derive aegis.build_id")?;
    if !sha_out.status.success() {
        bail!(
            "aegis.build_id not set and `git rev-parse --short HEAD` failed; \
             set `aegis.build_id` explicitly in the config"
        );
    }
    let sha = String::from_utf8_lossy(&sha_out.stdout).trim().to_string();
    let dirty = Command::new("git")
        .current_dir(root)
        .args(["status", "--porcelain"])
        .output()
        .ok()
        .map(|o| !o.stdout.is_empty())
        .unwrap_or(false);
    Ok(format!(
        "{prefix}-{sha}{}",
        if dirty { "-dirty" } else { "" }
    ))
}

pub(crate) fn load_cfg(root: &Path) -> Result<InfinityMsfsToml> {
    let cfg_path = util::config_path(root);
    if cfg_path.exists() {
        InfinityMsfsToml::load(&cfg_path)
    } else {
        Ok(InfinityMsfsToml::default())
    }
}

fn build_one(
    root: &Path,
    plan: &BuildPlan,
    wasm_opt_passes: &[String],
    use_wasm_opt: bool,
    release: bool,
    runner: &CliRunner,
    ui: &mut BuildUi,
    stats_db: &mut Stats,
) -> Result<BuildOutcome> {
    let built = built_artifact_path(root, plan, release);
    let final_path = plan.output_dir.join(&plan.artifact_name);

    ui.set_phase(&plan.package, BuildPhase::Compiling);
    run_cargo_build(runner, root, plan, release).map_err(|e| anyhow::anyhow!(e.to_string()))?;

    if !built.exists() {
        bail!(
            "cargo build completed, but built artifact was not found at {}",
            built.display()
        );
    }

    fs::create_dir_all(&plan.output_dir).with_context(|| {
        format!(
            "failed to create output directory {}",
            plan.output_dir.display()
        )
    })?;

    let run_opt = use_wasm_opt && plan.kind == ArtifactKind::Wasm;
    if run_opt {
        ui.set_phase(&plan.package, BuildPhase::Optimizing);
        run_wasm_opt(runner, root, wasm_opt_passes, &built, &final_path)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    } else {
        ui.set_phase(&plan.package, BuildPhase::Copying);
        util::copy_file(&built, &final_path)?;
    }

    ui.set_phase(&plan.package, BuildPhase::Copying);
    let mut copied_files = run_copy_rules(root, &plan.copy_files)?;
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
