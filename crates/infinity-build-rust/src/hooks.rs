use infinity_build_core::{BuildResult, Runner};
use std::{path::Path, process::Command};

pub fn run_hook_list(
    runner: &dyn Runner,
    root: &Path,
    label: &str,
    scripts: &[String],
) -> BuildResult<()> {
    for (i, script) in scripts.iter().enumerate() {
        let mut cmd = shell_command(script);
        cmd.current_dir(root);
        runner.run(&mut cmd, &format!("hook {label}[{i}]"))?;
    }
    Ok(())
}

#[cfg(windows)]
fn shell_command(script: &str) -> Command {
    let mut cmd = Command::new("cmd");
    cmd.arg("/C").arg(script);
    cmd
}

#[cfg(not(windows))]
fn shell_command(script: &str) -> Command {
    let mut cmd = Command::new("sh");
    cmd.arg("-c").arg(script);
    cmd
}
