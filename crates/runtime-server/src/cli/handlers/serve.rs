use anyhow::{Result, bail};

use crate::cli::args::{Cli, ServeArgs};
use crate::cli::context::resolve_workspace;
use crate::runtime_serve::{RuntimeServeCli, run};

pub async fn run_serve(cli: &Cli, args: ServeArgs) -> Result<()> {
    let selected = [args.mcp, args.http, args.acp]
        .into_iter()
        .filter(|v| *v)
        .count();
    if selected != 1 {
        bail!("Choose exactly one server mode: --http (MVP), --mcp, or --acp");
    }
    if args.mcp || args.acp {
        bail!("Only `zagens serve --http` is supported in this release");
    }

    if args.host != "127.0.0.1" && args.host != "localhost" {
        eprintln!(
            "⚠ zagens serve --http is binding to {} (not localhost).\n\
             The runtime API will be reachable from other machines on the network.\n\
             Make sure you have set --auth-token (or DEEPSEEK_RUNTIME_TOKEN) and\n\
             configured restrictive CORS origins via --cors-origin or config.toml.",
            args.host,
        );
    }

    let serve_cli = RuntimeServeCli {
        config: cli.config.clone(),
        profile: cli.profile.clone(),
        workspace: Some(resolve_workspace(cli)),
        host: args.host,
        port: args.port,
        workers: args.workers,
        cors_origin: args.cors_origin,
        auth_token: args.auth_token,
        verbose: cli.verbose,
    };
    run(serve_cli).await
}
