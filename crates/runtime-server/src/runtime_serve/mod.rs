//! HTTP sidecar entry for Zagens and headless hosts (D6 `runtime-server`).

mod http;

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;

use crate::cli::configure_windows_console_utf8;
use crate::config::Config;

pub use http::{run_http_server, RuntimeApiOptions};

/// CLI for the `deepseek-runtime` sidecar binary (HTTP only — no ratatui / full CLI).
#[derive(Parser, Debug)]
#[command(
    name = "deepseek-runtime",
    about = "DeepSeek runtime HTTP/SSE sidecar (Zagens desktop; no TUI)",
    version
)]
pub struct RuntimeServeCli {
    /// Config file path (default: ~/.deepseek/config.toml)
    #[arg(short, long)]
    pub config: Option<PathBuf>,
    /// Config profile name
    #[arg(long)]
    pub profile: Option<String>,
    /// Workspace root for tool execution
    #[arg(short, long)]
    pub workspace: Option<PathBuf>,
    /// Bind host (default localhost)
    #[arg(long, default_value = "127.0.0.1")]
    pub host: String,
    /// Bind port (`0` = ephemeral)
    #[arg(long, default_value_t = 7878)]
    pub port: u16,
    /// Background task worker count (1-16)
    #[arg(long, default_value_t = 8)]
    pub workers: usize,
    /// Additional CORS origin (repeatable)
    #[arg(long = "cors-origin", value_name = "URL")]
    pub cors_origin: Vec<String>,
    /// Bearer token for `/v1/*` (also reads `DEEPSEEK_RUNTIME_TOKEN`)
    #[arg(long = "auth-token", value_name = "TOKEN")]
    pub auth_token: Option<String>,
    /// Verbose logging
    #[arg(short, long)]
    pub verbose: bool,
}

/// Resolve additional CORS origins from flags, env, and config.
pub fn resolve_cors_origins(config: &Config, flag_origins: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |raw: &str| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return;
        }
        if !out.iter().any(|existing| existing == trimmed) {
            out.push(trimmed.to_string());
        }
    };
    for o in flag_origins {
        push(o);
    }
    if let Ok(env_value) = std::env::var("DEEPSEEK_CORS_ORIGINS") {
        for piece in env_value.split(',') {
            push(piece);
        }
    }
    if let Some(rt) = &config.runtime_api
        && let Some(list) = &rt.cors_origins
    {
        for o in list {
            push(o);
        }
    }
    out
}

fn load_config(cli: &RuntimeServeCli) -> Result<Config> {
    let profile = cli
        .profile
        .clone()
        .or_else(|| std::env::var("DEEPSEEK_PROFILE").ok());
    Config::load(cli.config.clone(), profile.as_deref()).context("load config")
}

fn resolve_workspace(cli: &RuntimeServeCli) -> PathBuf {
    cli.workspace.clone().unwrap_or_else(|| {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    })
}

/// Run the HTTP runtime sidecar until shutdown.
pub async fn run(cli: RuntimeServeCli) -> Result<()> {
    configure_windows_console_utf8();
    crate::logging::set_verbose(cli.verbose || crate::logging::env_requests_verbose_logging());

    if cli.host != "127.0.0.1" && cli.host != "localhost" {
        eprintln!(
            "⚠ deepseek-runtime is binding to {} (not localhost).\n\
             The runtime API will be reachable from other machines on the network.\n\
             Make sure you have set --auth-token (or DEEPSEEK_RUNTIME_TOKEN) and\n\
             configured restrictive CORS origins via --cors-origin or config.toml.",
            cli.host,
        );
    }

    let config = load_config(&cli)?;
    let workspace = resolve_workspace(&cli);
    let skills_dir = config.skills_dir();
    tokio::spawn(async move {
        if let Err(e) = crate::skills::install_system_skills(&skills_dir) {
            crate::logging::warn(format!("Failed to install system skills: {e}"));
        }
    });

    let cors_origins = resolve_cors_origins(&config, &cli.cors_origin);
    run_http_server(
        config,
        workspace,
        RuntimeApiOptions {
            host: cli.host,
            port: cli.port,
            workers: cli.workers.clamp(1, 16),
            cors_origins,
            auth_token: cli.auth_token,
        },
    )
    .await
}

/// Binary `main` helper — maps clean shutdown to exit code 0.
pub async fn run_or_exit(cli: RuntimeServeCli) -> ! {
    match run(cli).await {
        Ok(()) => {
            eprintln!("[deepseek-runtime] server shut down cleanly, exiting");
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("[deepseek-runtime] fatal: {:#}", e);
            std::process::exit(1);
        }
    }
}

/// Parse argv and run — used by the `deepseek-runtime` binary.
pub async fn run_from_args<I, T>(args: I) -> !
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let cli = RuntimeServeCli::parse_from(args);
    run_or_exit(cli).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_cors_dedupes() {
        let config = Config::default();
        let out = resolve_cors_origins(&config, &["http://a".into(), "http://a".into()]);
        assert_eq!(out, vec!["http://a".to_string()]);
    }
}
