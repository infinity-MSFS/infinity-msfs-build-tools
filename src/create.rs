use crate::cli::CreateArgs;
use anyhow::Result;
use infinity_build_create::{CreateOptions, run};

pub fn run_create(args: CreateArgs) -> Result<()> {
    run(CreateOptions {
        path: args.path,
        template: args.template,
        name: args.name,
        no_input: args.no_input,
        force: args.force,
    })
}
