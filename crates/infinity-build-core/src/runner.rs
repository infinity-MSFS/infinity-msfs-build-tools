use crate::BuildResult;
use std::process::Command;

pub trait Runner: Send + Sync {
    fn run(&self, cmd: &mut Command, label: &str) -> BuildResult<()>;
}
