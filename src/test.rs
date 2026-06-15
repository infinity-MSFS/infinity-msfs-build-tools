use crate::{build, cli::TestArgs, util};
use anyhow::{Context, Result, bail};
use console::style;
use std::process::Command;

/// Cargo target-selector flags. If the user passes one of these, we don't
/// inject the default `--lib`.
const TARGET_SELECTORS: &[&str] = &[
    "--lib",
    "--bin",
    "--bins",
    "--example",
    "--examples",
    "--test",
    "--tests",
    "--bench",
    "--benches",
    "--doc",
    "--all-targets",
];

/// Run `cargo test` for the configured rust packages on the host triple.
///
/// A thin passthrough: it resolves which `[[rust.packages]]` to test (all of
/// them, or the `--only` subset), then hands off to `cargo test` with any
/// trailing `cargo_args` forwarded verbatim. No `--target` is set, so tests
/// build for the host.
///
/// It defaults to `--lib` (unit tests only): the gauge packages are `cdylib`s
/// whose `nvg*`/`fs*` imports only resolve against the sim/emulator, so a full
/// `cargo test` — which links the `cdylib` and builds doctests against it —
/// can't link on the host. Unit tests build the crate with `cfg(test)`, where
/// the sim-linked layer is gated out. Pass an explicit target selector
/// (`--tests`, `--all-targets`, …) to override.
pub fn run_test(args: TestArgs) -> Result<()> {
    let root = util::find_project_root()?;
    let cfg = build::load_cfg(&root)?;

    // The configured packages, narrowed by `--only`. When none are configured
    // we fall back to a plain workspace `cargo test` (no `-p`).
    let mut packages: Vec<String> =
        cfg.rust.packages.iter().map(|p| p.cargo_package.clone()).collect();
    if !args.only.is_empty() {
        let before = packages.len();
        packages.retain(|p| args.only.iter().any(|o| o == p));
        if packages.is_empty() {
            bail!(
                "no [[rust.packages]] entry matched filter {:?} (had {before} candidate{})",
                args.only,
                if before == 1 { "" } else { "s" }
            );
        }
    }

    // Don't inject `--lib` if the caller picked their own target(s).
    let user_selected_target = args.cargo_args.iter().any(|a| {
        TARGET_SELECTORS
            .iter()
            .any(|s| a == s || a.strip_prefix(s).is_some_and(|r| r.starts_with('=')))
    });

    let mut cargo_args: Vec<String> = vec!["test".into()];
    for p in &packages {
        cargo_args.push("-p".into());
        cargo_args.push(p.clone());
    }
    if args.release {
        cargo_args.push("--release".into());
    }
    if !user_selected_target {
        cargo_args.push("--lib".into());
    }
    cargo_args.extend(args.cargo_args.iter().cloned());

    eprintln!(
        "{} cargo {}",
        style("running").cyan().bold(),
        cargo_args.join(" ")
    );

    // Inherit stdio so libtest's results stream live — capturing them would
    // defeat the point of running tests.
    let status = Command::new("cargo")
        .current_dir(&root)
        .args(&cargo_args)
        .status()
        .context("failed to start cargo test")?;
    if !status.success() {
        bail!("cargo test failed with status {status}");
    }
    Ok(())
}
