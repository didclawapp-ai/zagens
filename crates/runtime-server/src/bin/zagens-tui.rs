//! Full-screen TUI entry (`zagens-tui`).

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use anyhow::Context;
    use clap::Parser;
    use zagens_runtime::cli::{Cli, configure_windows_console_utf8};

    dotenvy::dotenv().ok();
    configure_windows_console_utf8();

    let cli = Cli::parse();
    zagens_runtime::tui::run_tui(cli)
        .await
        .context("zagens-tui failed")
}
