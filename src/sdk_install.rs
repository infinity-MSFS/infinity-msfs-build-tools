use crate::cli::SdkInstallArgs;
use anyhow::Result;
use infinity_build_sdk::{
    InstallOptions, run_install as crate_install, run_path as crate_path,
    run_remove as crate_remove,
};

pub fn run_install(args: SdkInstallArgs) -> Result<()> {
    crate_install(InstallOptions { force: args.force })
}

pub fn run_path() -> Result<()> {
    crate_path()
}

pub fn run_remove() -> Result<()> {
    crate_remove()
}
