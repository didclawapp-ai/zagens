use anyhow::Result;

use crate::cli::args::SetupArgs;
use crate::cli::context::CliContext;
use crate::cli::setup::{run_setup as run_setup_impl, run_setup_status};

pub fn run(ctx: &CliContext, args: SetupArgs) -> Result<()> {
    run_setup_impl(&ctx.config, &ctx.workspace, args)
}

pub fn run_status(ctx: &CliContext) -> Result<()> {
    run_setup_status(&ctx.config, &ctx.workspace)
}
