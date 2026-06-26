use anyhow::{Result, bail};

use crate::cli::args::{Cli, Commands};
use crate::cli::context::load_cli_context;
use crate::cli::handlers;

pub async fn run(cli: Cli) -> Result<()> {
    crate::logging::set_verbose(cli.verbose || crate::logging::env_requests_verbose_logging());
    let ctx = load_cli_context(&cli)?;
    let config_path = cli.config.as_deref();

    match cli.command.clone().expect("subcommand checked in main") {
        Commands::Doctor(args) => handlers::doctor::run(&ctx, config_path, args).await,
        Commands::OpenUrl(args) => handlers::open_url::run(args),
        Commands::Setup(args) => handlers::setup::run(&ctx, args),
        Commands::Completions { shell } => {
            handlers::completions::run(shell);
            Ok(())
        }
        Commands::Login { api_key } => handlers::login::run_login(api_key),
        Commands::Logout => handlers::login::run_logout(),
        Commands::Models(args) => handlers::models::run(&ctx, args).await,
        Commands::Exec(args) => handlers::exec::run(&cli, &ctx, args).await,
        Commands::Review(args) => handlers::review::run_review(&ctx, args).await,
        Commands::Apply(args) => handlers::review::run_apply(&ctx.workspace, args),
        Commands::CoverageGate(args) => {
            let code = handlers::coverage_gate::run(args)?;
            std::process::exit(if code == std::process::ExitCode::SUCCESS {
                0
            } else {
                1
            });
        }
        Commands::Trace { command } => {
            let ctx = load_cli_context(&cli)?;
            match command {
                crate::cli::args::TraceCommand::Export(args) => {
                    let code = handlers::trace::run(&ctx, args)?;
                    std::process::exit(if code == std::process::ExitCode::SUCCESS {
                        0
                    } else {
                        1
                    });
                }
                crate::cli::args::TraceCommand::Compare(args) => {
                    let code = handlers::trace_compare::run(&ctx, args)?;
                    std::process::exit(if code == std::process::ExitCode::SUCCESS {
                        0
                    } else {
                        1
                    });
                }
                crate::cli::args::TraceCommand::Serve(args) => {
                    handlers::trace_serve::run(&ctx, args).await?;
                    Ok(())
                }
            }
        }
        Commands::Serve(args) => handlers::serve::run_serve(&cli, args).await,
        Commands::Mcp { command } => handlers::mcp::run_mcp(&ctx, command).await,
        Commands::Sessions { .. }
        | Commands::Init
        | Commands::Pr { .. }
        | Commands::Eval(_)
        | Commands::Execpolicy(_)
        | Commands::Features(_)
        | Commands::Resume { .. }
        | Commands::Fork { .. } => {
            bail!("This subcommand is not available in headless CLI yet — see TUI方案.md")
        }
        Commands::Sandbox(args) => handlers::sandbox::run(args.command),
    }
}
