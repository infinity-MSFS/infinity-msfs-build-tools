use crate::process;
use anyhow::Result;
use std::{path::Path, process::Command};

pub fn run_script_list(root: &Path, label: &str, scripts: &[String], verbose: bool) -> Result<()> {
    for (i, script) in scripts.iter().enumerate() {
        run_script(root, &format!("{label}[{i}]"), script, verbose)?;
    }

    Ok(())
}

fn run_script(root: &Path, label: &str, script: &str, verbose: bool) -> Result<()> {
    let mut cmd = shell_command(script);
    cmd.current_dir(root);

    process::run_command(&mut cmd, &format!("script {label}"), verbose)?;
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
