use anyhow::{Context, Result, bail};
use console::style;
use std::process::Command;

/// Maximum number of trailing lines to surface from stderr when no
/// `error[…]` block is found. Keeps the failure message scannable
/// instead of dumping every cargo "Compiling foo" line.
const MAX_TAIL_LINES: usize = 40;

pub fn run_command(cmd: &mut Command, label: &str, verbose: bool) -> Result<()> {
    if verbose {
        let status = cmd
            .status()
            .with_context(|| format!("failed to start {label}"))?;

        if !status.success() {
            bail!("{label} failed with status {status}");
        }

        return Ok(());
    }

    let output = cmd
        .output()
        .with_context(|| format!("failed to start {label}"))?;

    let stdout = decode_output(&output.stdout);
    let stderr = decode_output(&output.stderr);

    if !output.status.success() {
        let mut message = format!("{label} failed with status {}", output.status);
        let rendered = render_failure(&stdout, &stderr);
        if !rendered.is_empty() {
            message.push_str("\n\n");
            message.push_str(&rendered);
        }
        message.push_str(&format!(
            "\n\n{} re-run with {} for full output.",
            style("hint:").yellow().bold(),
            style("-v").yellow(),
        ));

        bail!(message);
    }

    Ok(())
}

fn decode_output(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).replace("\r\n", "\n")
}

/// Pull the most useful slice out of a failed command's output.
///
/// Cargo writes diagnostics to stderr in a structured `error[E…]: …`
/// form. When such a block is present we print stderr starting from
/// the first such marker (so the user sees the real error first, not a
/// wall of `Compiling x v0.1.0` noise). Otherwise we fall back to the
/// last `MAX_TAIL_LINES` lines, which is enough to spot most linker
/// failures and panics without flooding the terminal.
fn render_failure(stdout: &str, stderr: &str) -> String {
    let mut sections = Vec::new();

    let stderr_trimmed = stderr.trim_end();
    if !stderr_trimmed.is_empty() {
        let body = focus_errors(stderr_trimmed);
        sections.push(format!("{}\n{}", style("stderr:").red().bold(), body));
    }

    let stdout_trimmed = stdout.trim_end();
    if !stdout_trimmed.is_empty() {
        let body = tail_lines(stdout_trimmed, MAX_TAIL_LINES);
        sections.push(format!("{}\n{}", style("stdout:").dim().bold(), body));
    }

    sections.join("\n\n")
}

fn focus_errors(text: &str) -> String {
    if let Some(idx) = find_error_marker(text) {
        let (skipped_prefix, rest) = text.split_at(idx);
        let elided = if skipped_prefix.is_empty() {
            String::new()
        } else {
            format!("{}\n", style("… (earlier output elided)").dim())
        };
        format!("{elided}{rest}")
    } else {
        tail_lines(text, MAX_TAIL_LINES)
    }
}

fn find_error_marker(text: &str) -> Option<usize> {
    // Look for cargo / rustc diagnostic markers at the start of a line.
    // `error[E…]:` and bare `error:` both qualify; `warning:` does not.
    let mut search_from = 0;
    while let Some(rel) = text[search_from..].find("error") {
        let abs = search_from + rel;
        let at_line_start = abs == 0 || text.as_bytes()[abs - 1] == b'\n';
        let after = &text[abs + "error".len()..];
        let looks_like_diagnostic = after.starts_with(':') || after.starts_with('[');
        if at_line_start && looks_like_diagnostic {
            return Some(abs);
        }
        search_from = abs + "error".len();
    }
    None
}

fn tail_lines(text: &str, max: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= max {
        return text.to_string();
    }
    let kept = &lines[lines.len() - max..];
    let elided = lines.len() - max;
    format!(
        "{}\n{}",
        style(format!("… ({elided} earlier lines elided)")).dim(),
        kept.join("\n"),
    )
}
