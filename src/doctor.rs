//! `infinity-msfs doctor` — pre-flight environment checks.
//!
//! Each check returns a [`Status`] which the runner renders with a
//! coloured glyph. The command always exits 0 unless something fatal
//! happens while running the checks themselves; informational warnings
//! are surfaced in the output but do not fail the program.

use crate::sdk::{cache_base, current_version};
use anyhow::Result;
use console::style;
use std::{path::PathBuf, process::Command};

#[derive(Clone, Copy)]
enum Status {
    Ok,
    Warn,
    Fail,
}

struct CheckResult {
    name: &'static str,
    status: Status,
    detail: String,
    hint: Option<String>,
}

pub fn run_doctor() -> Result<()> {
    println!(
        "{} {}",
        style("Checking").cyan().bold(),
        style("infinity-msfs build environment").dim(),
    );
    println!();

    let checks: Vec<CheckResult> = vec![
        check_cargo(),
        check_wasm_target(),
        check_wasm_opt(),
        check_sdk(),
        check_simconnect_lib(),
    ];

    let pad = checks.iter().map(|c| c.name.len()).max().unwrap_or(0);

    let mut warnings = 0usize;
    let mut failures = 0usize;
    for check in &checks {
        let glyph = match check.status {
            Status::Ok => style("✓").green().bold().to_string(),
            Status::Warn => {
                warnings += 1;
                style("!").yellow().bold().to_string()
            }
            Status::Fail => {
                failures += 1;
                style("✗").red().bold().to_string()
            }
        };
        println!(
            "  {glyph} {:<width$}  {}",
            style(check.name).bold(),
            style(&check.detail).dim(),
            width = pad,
        );
        if let Some(hint) = &check.hint {
            println!("    {} {}", style("→").dim(), hint);
        }
    }

    println!();
    let summary = if failures > 0 {
        style(format!("{failures} check(s) failed, {warnings} warning(s)"))
            .red()
            .bold()
    } else if warnings > 0 {
        style(format!("ok with {warnings} warning(s)"))
            .yellow()
            .bold()
    } else {
        style("all checks passed".to_string()).green().bold()
    };
    println!("{summary}");

    Ok(())
}

// ---------------------------------------------------------------------------
// Individual checks
// ---------------------------------------------------------------------------

fn check_cargo() -> CheckResult {
    match Command::new("cargo").arg("--version").output() {
        Ok(out) if out.status.success() => CheckResult {
            name: "cargo",
            status: Status::Ok,
            detail: String::from_utf8_lossy(&out.stdout).trim().to_string(),
            hint: None,
        },
        _ => CheckResult {
            name: "cargo",
            status: Status::Fail,
            detail: "not found on PATH".to_string(),
            hint: Some("install Rust: https://rustup.rs".to_string()),
        },
    }
}

fn check_wasm_target() -> CheckResult {
    let out = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output();
    match out {
        Ok(out) if out.status.success() => {
            let installed = String::from_utf8_lossy(&out.stdout);
            if installed.lines().any(|l| l.trim() == "wasm32-wasip1") {
                CheckResult {
                    name: "wasm32-wasip1",
                    status: Status::Ok,
                    detail: "installed".to_string(),
                    hint: None,
                }
            } else {
                CheckResult {
                    name: "wasm32-wasip1",
                    status: Status::Fail,
                    detail: "target not installed".to_string(),
                    hint: Some("rustup target add wasm32-wasip1".to_string()),
                }
            }
        }
        _ => CheckResult {
            name: "wasm32-wasip1",
            status: Status::Warn,
            detail: "could not query rustup".to_string(),
            hint: Some(
                "ensure rustup is installed; the target is required for WASM builds".to_string(),
            ),
        },
    }
}

fn check_wasm_opt() -> CheckResult {
    match Command::new("wasm-opt").arg("--version").output() {
        Ok(out) if out.status.success() => CheckResult {
            name: "wasm-opt",
            status: Status::Ok,
            detail: String::from_utf8_lossy(&out.stdout).trim().to_string(),
            hint: None,
        },
        _ => CheckResult {
            name: "wasm-opt",
            status: Status::Warn,
            detail: "not found on PATH".to_string(),
            hint: Some(
                "install Binaryen, or set [wasm_opt] enabled = false in infinity-msfs.toml"
                    .to_string(),
            ),
        },
    }
}

fn check_sdk() -> CheckResult {
    if let Ok(sdk) = std::env::var("MSFS2024_SDK") {
        return CheckResult {
            name: "MSFS SDK",
            status: Status::Ok,
            detail: format!("MSFS2024_SDK = {sdk}"),
            hint: None,
        };
    }

    let Some(base) = cache_base() else {
        return CheckResult {
            name: "MSFS SDK",
            status: Status::Warn,
            detail: "could not resolve a cache directory".to_string(),
            hint: Some("set INFINITY_MSFS_SDK_CACHE or MSFS2024_SDK to a valid path".to_string()),
        };
    };

    let Some(version) = current_version(&base) else {
        return CheckResult {
            name: "MSFS SDK",
            status: Status::Fail,
            detail: format!("no SDK installed under {}", base.display()),
            hint: Some("run `infinity-msfs sdk install`".to_string()),
        };
    };

    let path = base.join(&version);
    if path.exists() {
        CheckResult {
            name: "MSFS SDK",
            status: Status::Ok,
            detail: format!("v{version} at {}", path.display()),
            hint: None,
        }
    } else {
        CheckResult {
            name: "MSFS SDK",
            status: Status::Fail,
            detail: format!("current.txt points to missing {}", path.display()),
            hint: Some("run `infinity-msfs sdk install --force`".to_string()),
        }
    }
}

fn check_simconnect_lib() -> CheckResult {
    let sdk_root: Option<PathBuf> = if let Ok(p) = std::env::var("MSFS2024_SDK") {
        Some(PathBuf::from(p))
    } else {
        cache_base().and_then(|b| current_version(&b).map(|v| b.join(v)))
    };

    let Some(root) = sdk_root else {
        return CheckResult {
            name: "SimConnect lib",
            status: Status::Warn,
            detail: "skipped — SDK location unknown".to_string(),
            hint: None,
        };
    };

    let lib_dir = root.join("SimConnect SDK").join("lib");
    let static_lib = lib_dir.join("static").join("SimConnect.lib");
    let dll = lib_dir.join("SimConnect.dll");

    if !lib_dir.exists() {
        return CheckResult {
            name: "SimConnect lib",
            status: Status::Warn,
            detail: "not present in SDK (only required for native + simconnect builds)".to_string(),
            hint: Some("re-run `infinity-msfs sdk install` to fetch the latest SDK".to_string()),
        };
    }

    let mut missing: Vec<&str> = Vec::new();
    if !static_lib.exists() {
        missing.push("lib/static/SimConnect.lib");
    }
    if !dll.exists() {
        missing.push("lib/SimConnect.dll");
    }

    if missing.is_empty() {
        CheckResult {
            name: "SimConnect lib",
            status: Status::Ok,
            detail: format!("found at {}", lib_dir.display()),
            hint: None,
        }
    } else {
        CheckResult {
            name: "SimConnect lib",
            status: Status::Warn,
            detail: format!("missing: {}", missing.join(", ")),
            hint: Some("native + simconnect builds will fail to link or run".to_string()),
        }
    }
}
