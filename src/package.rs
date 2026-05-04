use crate::{cli::PackageArgs, config::InfinityMsfsToml, runner::CliRunner, util};
use anyhow::{Result, bail};
use console::style;
use infinity_build_package::{PackageOverrides, SimPackage, build_one, locate_fspackagetool};
use infinity_build_sdk as sdk;
use std::time::Instant;

pub fn run_package(args: PackageArgs) -> Result<()> {
    if !cfg!(target_os = "windows") {
        bail!(
            "`infinity-msfs package` is Windows-only.\n\
             fspackagetool.exe drives the MSFS 2024 sim binary to compile assets,\n\
             and that binary only ships for Windows."
        );
    }

    sdk::ensure_sdk()?;

    let root = util::find_project_root()?;
    let cfg_path = util::config_path(&root);
    if !cfg_path.exists() {
        bail!(
            "no infinity-msfs.toml found at {}; run `infinity-msfs package` from a project root",
            cfg_path.display()
        );
    }

    let cfg = InfinityMsfsToml::load(&cfg_path)?;
    if cfg.sim_packages.is_empty() {
        bail!(
            "no [[sim_packages]] entries in {}.\n\
             Add at least one entry pointing at a project .xml.",
            cfg_path.display()
        );
    }

    let selected: Vec<&SimPackage> = if args.only.is_empty() {
        cfg.sim_packages.iter().collect()
    } else {
        let chosen: Vec<&SimPackage> = cfg
            .sim_packages
            .iter()
            .filter(|p| args.only.iter().any(|n| n == &p.name))
            .collect();
        if chosen.is_empty() {
            bail!(
                "no [[sim_packages]] entry matched filter {:?} (available: {})",
                args.only,
                cfg.sim_packages
                    .iter()
                    .map(|p| p.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        chosen
    };

    let tool = locate_fspackagetool()?;
    let runner = CliRunner {
        verbose: args.verbose,
    };
    let overrides = PackageOverrides {
        force_rebuild: args.rebuild,
        mirror_output: args.mirror,
        prefer_steam: args.force_steam,
        marketplace_dir: args.marketplace.clone(),
    };

    println!(
        "{} {} {} {} {}",
        style("Packaging").cyan().bold(),
        style(selected.len()).bold(),
        if selected.len() == 1 {
            "project"
        } else {
            "projects"
        },
        style("via").dim(),
        style(tool.display()).dim(),
    );

    let started = Instant::now();
    let mut succeeded = 0usize;
    let mut failed: Vec<String> = Vec::new();

    for entry in &selected {
        let entry_started = Instant::now();
        match build_one(&runner, &root, &tool, entry, &overrides) {
            Ok(()) => {
                succeeded += 1;
                println!(
                    "{} {} {}",
                    style("✓").green().bold(),
                    style(&entry.name).bold(),
                    style(format!("({:.1?})", entry_started.elapsed())).dim(),
                );
            }
            Err(e) => {
                failed.push(entry.name.clone());
                eprintln!(
                    "{} {} {}",
                    style("✗").red().bold(),
                    style(&entry.name).bold(),
                    style(format!("{e:#}")).dim(),
                );
            }
        }
    }

    println!(
        "{} packaged {}/{} in {:.1?}{}",
        style("Done").green().bold(),
        style(succeeded).bold(),
        style(selected.len()).bold(),
        started.elapsed(),
        if failed.is_empty() {
            String::new()
        } else {
            format!(" — failed: {}", failed.join(", "))
        },
    );

    if !failed.is_empty() {
        bail!("{} package(s) failed", failed.len());
    }
    Ok(())
}
