use anyhow::Error;
use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub struct BuildUi {
    progress: ProgressBar,
    started: Instant,
    package_started: Option<Instant>,
    total: usize,
    copied_files: usize,
    verbose: bool,
    plain: bool,
}

pub struct BuildOutcome {
    pub copied_files: usize,
    pub size_bytes: Option<u64>,
    pub previous_size_bytes: Option<u64>,
}

#[derive(Clone, Copy)]
pub enum BuildPhase {
    Compiling,
    Optimizing,
    Copying,
}

impl BuildPhase {
    fn label(self) -> &'static str {
        match self {
            BuildPhase::Compiling => "compiling",
            BuildPhase::Optimizing => "optimizing",
            BuildPhase::Copying => "copying",
        }
    }
}

fn is_plain_output() -> bool {
    use std::io::IsTerminal;
    if !std::io::stdout().is_terminal() {
        return true;
    }
    for var in [
        "CI",
        "GITHUB_ACTIONS",
        "TF_BUILD",
        "BUILDKITE",
        "TEAMCITY_VERSION",
    ] {
        if std::env::var_os(var).is_some() {
            return true;
        }
    }
    false
}

impl BuildUi {
    pub fn new(root: &Path, total: usize, release: bool, wasm_opt: bool, verbose: bool) -> Self {
        println!(
            "{} {} {} {} {}",
            style("Building").cyan().bold(),
            style(total).bold(),
            pluralize(total, "package"),
            style(format!(
                "({}; wasm-opt {})",
                if release { "release" } else { "debug" },
                if wasm_opt { "on" } else { "off" }
            ))
            .dim(),
            style(root.display()).dim()
        );

        let plain = verbose || is_plain_output();

        let progress = ProgressBar::new(total as u64);
        progress.set_style(
            ProgressStyle::with_template(
                "{spinner:.cyan} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len} {msg}",
            )
            .expect("valid build progress template")
            .progress_chars("=> "),
        );

        if plain {
            progress.set_draw_target(indicatif::ProgressDrawTarget::hidden());
        } else {
            progress.enable_steady_tick(Duration::from_millis(120));
        }

        Self {
            progress,
            started: Instant::now(),
            package_started: None,
            total,
            copied_files: 0,
            verbose,
            plain,
        }
    }

    pub fn announce_phase(&self, label: &str, count: usize) {
        if count == 0 {
            return;
        }

        let mut line = format!(
            "{} {} {}",
            style("→").cyan().bold(),
            style(label).bold(),
            style(format!("({count} {})", pluralize(count, "script"))).dim(),
        );
        if self.verbose {
            line.push(' ');
            line.push_str(&style("verbose").yellow().dim().to_string());
        }
        self.println(line);
    }

    pub fn start_package(&mut self, package: &str) {
        self.package_started = Some(Instant::now());
        self.set_phase(package, BuildPhase::Compiling);
    }

    pub fn set_phase(&self, package: &str, phase: BuildPhase) {
        self.progress
            .set_message(format!("{} {package}", phase.label()));
    }

    pub fn finish_package(&mut self, package: &str, output: &Path, outcome: BuildOutcome) {
        self.copied_files += outcome.copied_files;
        let elapsed = self
            .package_started
            .take()
            .map(|t| t.elapsed())
            .unwrap_or_default();

        self.progress.inc(1);
        self.println(format!(
            "{} {} {} {}{}",
            style("✓").green().bold(),
            style(package).bold(),
            style(shorten_path(output)).dim(),
            format_metrics(&outcome, elapsed),
            format_suffix(outcome.copied_files),
        ));
    }

    pub fn finish(self) {
        self.progress.finish_and_clear();

        let mut summary = format!(
            "{} built {} {} in {}",
            style("Done").green().bold(),
            style(self.total).bold(),
            pluralize(self.total, "package"),
            style(format_duration(self.started.elapsed())).dim()
        );

        if self.copied_files > 0 {
            summary.push_str(&format!(
                ", {} {}",
                style(self.copied_files).cyan().bold(),
                pluralize(self.copied_files, "copied file")
            ));
        }

        println!("{summary}");
    }

    fn println(&self, line: String) {
        if self.plain {
            println!("{line}");
        } else {
            self.progress.println(line);
        }
    }
}

fn format_metrics(outcome: &BuildOutcome, elapsed: Duration) -> String {
    let mut parts: Vec<String> = Vec::new();

    if let Some(size) = outcome.size_bytes {
        let mut size_text = format_bytes(size);
        if let Some(previous) = outcome.previous_size_bytes
            && previous != size
        {
            let delta = size as i64 - previous as i64;
            let sign = if delta > 0 { "+" } else { "-" };
            let styled = format!("{sign}{}", format_bytes(delta.unsigned_abs()));
            let coloured = if delta > 0 {
                style(styled).yellow().to_string()
            } else {
                style(styled).green().to_string()
            };
            size_text = format!("{size_text} {coloured}");
        }
        parts.push(size_text);
    }

    if elapsed > Duration::ZERO {
        parts.push(format_duration(elapsed));
    }

    if parts.is_empty() {
        String::new()
    } else {
        format!(" {}", style(format!("({})", parts.join(" · "))).dim())
    }
}

fn format_bytes(bytes: u64) -> String {
    const MB: u64 = 1024 * 1024;
    const KB: u64 = 1024;
    if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

pub fn print_projects(
    root: &Path,
    rust_projects: impl IntoIterator<Item = (String, String, String, PathBuf)>,
    js_instruments: impl IntoIterator<Item = (String, String)>,
) {
    println!(
        "{} {}",
        style("Projects").cyan().bold(),
        style(root.display()).dim()
    );

    let mut printed_rust_header = false;
    for (package, bin, target, output) in rust_projects {
        if !printed_rust_header {
            println!("  {}", style("rust").dim().bold());
            printed_rust_header = true;
        }
        println!(
            "    {} {} {} {} {}",
            style("•").cyan(),
            style(package).bold(),
            style(format!("[bin: {bin}]")).dim(),
            style(format!("[target: {target}]")).dim(),
            style(shorten_path(&output)).dim()
        );
    }

    let mut printed_js_header = false;
    for (name, index) in js_instruments {
        if !printed_js_header {
            if printed_rust_header {
                println!();
            }
            println!("  {}", style("js").dim().bold());
            printed_js_header = true;
        }
        println!(
            "    {} {} {}",
            style("•").cyan(),
            style(name).bold(),
            style(format!("[index: {index}]")).dim(),
        );
    }
}

pub fn announce_pre_hook(count: usize) {
    if count == 0 {
        return;
    }
    println!(
        "{} {} {}",
        style("→").cyan().bold(),
        style("Running pre-build hooks").bold(),
        style(format!("({count} {})", pluralize(count, "hook"))).dim(),
    );
}

pub fn announce_post_hook(count: usize) {
    if count == 0 {
        return;
    }
    println!(
        "{} {} {}",
        style("→").cyan().bold(),
        style("Running post-build hooks").bold(),
        style(format!("({count} {})", pluralize(count, "hook"))).dim(),
    );
}

pub fn print_error(err: &Error) {
    eprintln!("{} {err:#}", style("error:").red().bold());
}

fn pluralize(count: usize, singular: &str) -> String {
    if count == 1 {
        singular.to_string()
    } else {
        format!("{singular}s")
    }
}

fn format_suffix(copied_files: usize) -> String {
    if copied_files == 0 {
        String::new()
    } else {
        format!(
            " {}",
            style(format!(
                "({} {})",
                style(copied_files).cyan().bold(),
                pluralize(copied_files, "copy")
            ))
            .dim()
        )
    }
}

fn shorten_path(path: &Path) -> String {
    let label = path
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_string)
        .unwrap_or_else(|| path.display().to_string());
    format!("-> {}", hyperlink_path(path, &label))
}

fn hyperlink_path(path: &Path, text: &str) -> String {
    if !supports_hyperlinks() {
        return text.to_string();
    }

    let absolute = std::fs::canonicalize(path)
        .ok()
        .unwrap_or_else(|| path.to_path_buf());

    let url = path_to_file_url(&absolute);
    format!("\x1b]8;;{url}\x1b\\{text}\x1b]8;;\x1b\\")
}

fn supports_hyperlinks() -> bool {
    use std::io::IsTerminal;
    if !std::io::stdout().is_terminal() {
        return false;
    }
    if std::env::var_os("CI").is_some() {
        return false;
    }
    if matches!(std::env::var("TERM").as_deref(), Ok("dumb")) {
        return false;
    }
    true
}

fn path_to_file_url(path: &Path) -> String {
    let raw = path.to_string_lossy();
    // Windows canonical paths come back as \\?\C:\… — strip the verbatim
    // prefix so the URL is what terminals/IDEs actually expect.
    let trimmed = raw.strip_prefix(r"\\?\").unwrap_or(&raw);
    let forward = trimmed.replace('\\', "/");

    let mut encoded = String::with_capacity(forward.len() + 8);
    for ch in forward.chars() {
        match ch {
            'A'..='Z'
            | 'a'..='z'
            | '0'..='9'
            | '/'
            | ':'
            | '-'
            | '_'
            | '.'
            | '~'
            | '!'
            | '$'
            | '&'
            | '\''
            | '('
            | ')'
            | '*'
            | '+'
            | ','
            | ';'
            | '='
            | '@' => encoded.push(ch),
            _ => {
                let mut buf = [0u8; 4];
                for byte in ch.encode_utf8(&mut buf).as_bytes() {
                    encoded.push_str(&format!("%{byte:02X}"));
                }
            }
        }
    }

    if encoded.starts_with('/') {
        format!("file://{encoded}")
    } else {
        // Windows drive-letter path (C:/…); file:// needs empty host + '/' before it.
        format!("file:///{encoded}")
    }
}

fn format_duration(duration: Duration) -> String {
    let secs = duration.as_secs();
    let millis = duration.subsec_millis();

    if secs == 0 {
        format!("{millis}ms")
    } else if secs < 60 {
        format!("{secs}.{millis:03}s")
    } else {
        let minutes = secs / 60;
        let seconds = secs % 60;
        format!("{minutes}m {seconds}s")
    }
}
