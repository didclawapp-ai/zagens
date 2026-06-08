//! Headless CLI entry (`zagens`) — subcommand dispatch without TUI.

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use anyhow::Context;
    use clap::Parser;
    use deepseek_runtime::cli::{Cli, configure_windows_console_utf8, dispatch};

    dotenvy::dotenv().ok();
    configure_windows_console_utf8();

    let cli = Cli::parse();

    if cli.command.is_none() {
        use clap::CommandFactory;
        Cli::command().print_help()?;
        eprintln!();
        std::process::exit(1);
    }

    dispatch::run(cli).await.context("zagens command failed")
}
