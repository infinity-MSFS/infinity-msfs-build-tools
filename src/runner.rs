use crate::process;
use infinity_build_core::{BuildError, BuildResult, Runner};
use std::process::Command;

pub struct CliRunner {
    pub verbose: bool,
}

impl Runner for CliRunner {
    fn run(&self, cmd: &mut Command, label: &str) -> BuildResult<()> {
        process::run_command(cmd, label, self.verbose).map_err(BuildError::Backend)
    }
}
