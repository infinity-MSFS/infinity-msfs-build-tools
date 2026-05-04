use crate::{
    build,
    cli::{BuildArgs, WatchArgs},
    config::InfinityMsfsToml,
    util,
};
use anyhow::Result;
use console::style;
use infinity_build_rust::cargo_meta;
use infinity_build_watch::{WatchSpec, run as run_watcher};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

pub fn run_watch(args: WatchArgs) -> Result<()> {
    let root = util::find_project_root()?;
    let cfg_path = util::config_path(&root);
    let cfg = if cfg_path.exists() {
        InfinityMsfsToml::load(&cfg_path)?
    } else {
        InfinityMsfsToml::default()
    };

    let watch_js = !args.rust_only && cfg.js.is_some();
    let roots = collect_watch_dirs(&root, &cfg, watch_js)?;
    let ignored = collect_ignored_dirs(&root, &cfg);

    let spec = WatchSpec {
        roots,
        ignored,
        debounce_ms: args.debounce,
    };

    let args = args.clone();
    run_watcher(&root, spec, move || run_once(&args))
}

fn run_once(args: &WatchArgs) {
    let build_args = BuildArgs {
        release: args.release,
        verbose: args.verbose,
        only: args.only.clone(),
        rust_only: args.rust_only,
        js_only: args.js_only,
        no_wasm_opt: args.no_wasm_opt,
        minify: false,
        sourcemap: None,
        skip_simulator_package: false,
    };
    if let Err(e) = build::run_build(build_args) {
        eprintln!("{} build failed: {e:#}", style("✗").red().bold());
    }
}

fn collect_watch_dirs(root: &Path, cfg: &InfinityMsfsToml, watch_js: bool) -> Result<Vec<PathBuf>> {
    let metadata = cargo_meta::load_metadata(root)?;
    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();

    for pkg in &metadata.packages {
        if !metadata.workspace_members.contains(&pkg.id) {
            continue;
        }
        let manifest = PathBuf::from(pkg.manifest_path.as_str());
        let Some(parent) = manifest.parent() else {
            continue;
        };
        let src = parent.join("src");
        if src.exists() {
            insert_unique(&mut dirs, &mut seen, src);
        }
        if manifest.exists() {
            insert_unique(&mut dirs, &mut seen, parent.to_path_buf());
        }
    }

    if watch_js {
        if let Some(js) = &cfg.js {
            for inst in &js.instruments {
                let abs = root.join(&inst.index);
                if let Some(parent) = abs.parent() {
                    if parent.exists() {
                        insert_unique(&mut dirs, &mut seen, parent.to_path_buf());
                    }
                }
            }
        }
    }

    if dirs.is_empty() {
        dirs.push(root.to_path_buf());
    }

    Ok(dirs)
}

fn insert_unique(dirs: &mut Vec<PathBuf>, seen: &mut HashSet<PathBuf>, dir: PathBuf) {
    if seen.insert(dir.clone()) {
        dirs.push(dir);
    }
}

fn collect_ignored_dirs(root: &Path, cfg: &InfinityMsfsToml) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = vec![root.join("target"), root.join(".git")];
    if !cfg.rust.output_dir.is_empty() {
        out.push(root.join(&cfg.rust.output_dir));
    }
    for p in &cfg.rust.packages {
        if let Some(d) = &p.output_dir {
            out.push(root.join(d));
        }
    }
    for sp in &cfg.sim_packages {
        if let Some(d) = &sp.output_dir {
            out.push(root.join(d));
        }
        if let Some(d) = &sp.temp_dir {
            out.push(root.join(d));
        }
    }
    out
}
