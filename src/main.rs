mod build;
mod build_js;
mod cli;
mod config;
mod create;
mod doctor;
mod package;
mod process;
mod runner;
mod sdk_install;
mod ui;
mod util;
mod watch;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands, SdkCommand};


fn main() {
    if let Err(err) = real_main() {
        ui::print_error(&err);
        std::process::exit(1);
    }
}

fn real_main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Build(args) => build::run_build(args)?,
        Commands::Projects(args) => build::run_projects(args)?,
        Commands::Sdk(args) => match args.command {
            SdkCommand::Install(a) => sdk_install::run_install(a)?,
            SdkCommand::Path => sdk_install::run_path()?,
            SdkCommand::Remove => sdk_install::run_remove()?,
        },
        Commands::Doctor => doctor::run_doctor()?,
        Commands::Package(args) => package::run_package(args)?,
        Commands::Watch(args) => watch::run_watch(args)?,
        Commands::Create(args) => create::run_create(args)?,
    }

    Ok(())
}
