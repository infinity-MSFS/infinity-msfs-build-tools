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

    let do_rust = !args.js_only;
    let do_js = !args.rust_only && cfg.js.is_some();

    let any_rust = do_rust && !cfg.rust.packages.is_empty();
    let any_js = do_js;

    if !any_rust && !any_js {
        bail!(
            "nothing to build. Add `[[rust.packages]]` or a `[js]` section to {}",
            util::config_path(&root).display()
        );
    }

    ui::announce_pre_hook(cfg.hooks.pre.len());
    hooks::run_hook_list(&runner, &root, "pre", &cfg.hooks.pre)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;

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

    ui::announce_post_hook(cfg.hooks.post.len());
    hooks::run_hook_list(&runner, &root, "post", &cfg.hooks.post)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    Ok(())
}

fn run_rust_pipeline(
    root: &Path,
    cfg: &InfinityMsfsToml,
    args: &BuildArgs,
    runner: &CliRunner,
) -> Result<()> {
    sdk::ensure_sdk()?;

    let metadata = cargo_meta::load_metadata(root)?;
    let mut plans = resolve_plans(root, &cfg.rust, &metadata, &args.only)?;

    if !cfg!(target_os = "windows") {
        plans.retain(|plan| {
            if plan.kind == ArtifactKind::Native {
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
        // Filter eliminated everything (only matched JS instruments).
        // Not an error — JS step still runs.
        return Ok(());
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

    let plans = resolve_plans(&root, &cfg.rust, &metadata, &args.only).unwrap_or_default();

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

fn load_cfg(root: &Path) -> Result<InfinityMsfsToml> {
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
