use anyhow::Result;

use crate::cli::args::{Cli, ExecArgs};
use crate::cli::context::CliContext;
use crate::cli::runner::{ExecOptions, run_exec};

pub async fn run(cli: &Cli, ctx: &CliContext, args: ExecArgs) -> Result<()> {
    let auto_mode = args.auto || cli.yolo;
    run_exec(
        ctx,
        ExecOptions {
            prompt: args.prompt,
            model: args.model,
            auto_mode,
            json_output: args.json,
            max_subagents: cli.max_subagents,
        },
    )
    .await
}
