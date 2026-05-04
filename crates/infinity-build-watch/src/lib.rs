use anyhow::{Context, Result};
use console::style;
use notify_debouncer_mini::{
    DebounceEventResult, Debouncer, new_debouncer,
    notify::{RecommendedWatcher, RecursiveMode},
};
use std::{
    path::{Path, PathBuf},
    sync::mpsc,
    time::{Duration, Instant},
};

pub struct WatchSpec {
    pub roots: Vec<PathBuf>,
    pub ignored: Vec<PathBuf>,
    pub debounce_ms: u64,
}

pub fn run<F>(project_root: &Path, spec: WatchSpec, mut on_change: F) -> Result<()>
where
    F: FnMut(),
{
    let dirs = dedup_nested(spec.roots);

    println!(
        "{} {} {}",
        style("Watching").cyan().bold(),
        style(format!("{} dir(s)", dirs.len())).bold(),
        style(format!("(debounce {}ms)", spec.debounce_ms)).dim(),
    );
    for d in &dirs {
        println!("  {} {}", style("•").cyan(), style(d.display()).dim());
    }

    let (tx, rx) = mpsc::channel::<DebounceEventResult>();
    let mut debouncer: Debouncer<RecommendedWatcher> =
        new_debouncer(Duration::from_millis(spec.debounce_ms), move |res| {
            let _ = tx.send(res);
        })
        .context("failed to start file watcher")?;

    for dir in &dirs {
        debouncer
            .watcher()
            .watch(dir, RecursiveMode::Recursive)
            .with_context(|| format!("failed to watch {}", dir.display()))?;
    }

    on_change();

    println!(
        "{} waiting for changes (Ctrl-C to stop)…",
        style("→").cyan().bold()
    );

    loop {
        let events = match rx.recv() {
            Ok(Ok(events)) => events,
            Ok(Err(err)) => {
                eprintln!("{} watcher: {err}", style("warning:").yellow().bold());
                continue;
            }
            Err(_) => break,
        };

        let relevant: Vec<&Path> = events
            .iter()
            .map(|e| e.path.as_path())
            .filter(|p| !is_ignored(p, &spec.ignored))
            .collect();

        if relevant.is_empty() {
            continue;
        }

        let trigger = relevant[0];
        println!(
            "\n{} change detected: {}{}",
            style("↻").cyan().bold(),
            style(short_path(project_root, trigger)).dim(),
            if relevant.len() > 1 {
                format!(
                    " {}",
                    style(format!("(+{} more)", relevant.len() - 1)).dim()
                )
            } else {
                String::new()
            },
        );

        let started = Instant::now();
        on_change();
        println!(
            "{} cycle done in {:.1?}",
            style("→").cyan().bold(),
            started.elapsed()
        );
    }

    Ok(())
}

fn dedup_nested(mut dirs: Vec<PathBuf>) -> Vec<PathBuf> {
    dirs.sort_by_key(|p| p.components().count());
    let mut out: Vec<PathBuf> = Vec::new();
    for d in dirs {
        if !out.iter().any(|kept| d.starts_with(kept)) {
            out.push(d);
        }
    }
    out
}

fn is_ignored(path: &Path, ignored: &[PathBuf]) -> bool {
    if ignored.iter().any(|d| path.starts_with(d)) {
        return true;
    }
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        if name.starts_with('.') || name.ends_with('~') || name.ends_with(".swp") {
            return true;
        }
    }
    false
}

fn short_path(root: &Path, p: &Path) -> String {
    p.strip_prefix(root)
        .map(|r| r.display().to_string())
        .unwrap_or_else(|_| p.display().to_string())
}
