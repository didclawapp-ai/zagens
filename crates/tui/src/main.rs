//! CLI entry point for the `DeepSeek` client.

use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use clap::{Args, CommandFactory, Parser, Subcommand};
use clap_complete::{Shell, generate};
use dotenvy::dotenv;
use tempfile::NamedTempFile;
use wait_timeout::ChildExt;

mod acp_server;
mod agent_surface;
mod audit;
mod auto_reasoning;
mod auto_route;
mod automation_manager;
mod client;
mod command_safety;
mod commands;
mod compaction;
mod context_snapshot;
mod composer_history;
mod composer_stash;
mod config;
mod context_reference;
mod config_ui;
mod core;
mod cost_status;
mod cycle_manager;
mod deepseek_theme;
mod error_taxonomy;
mod eval;
mod execpolicy;
mod features;
mod handoff;
mod hooks;
mod json_schema_util;
mod llm_client;
mod localization;
mod logging;
mod lsp;
mod mcp;
mod mcp_server;
mod memory;
mod topic_memory;
mod models;
mod network_policy;
mod palette;
mod path_guard;
mod pricing;
mod project_context;
mod project_doc;
mod prompts;
mod python_env;
pub mod repl;
mod retry_status;
pub mod rlm;
mod runtime_api;
mod runtime_threads;
mod sandbox;
mod schema_migration;
mod scratchpad;
mod seam_manager;
mod session_manager;
mod session_store_sqlite;
mod thread_store_sqlite;
mod settings;
mod skills;
mod task_type;
mod snapshot;
mod symbol_index;
mod task_manager;
#[cfg(test)]
mod test_support;
mod tools;
mod tui;
mod utils;
mod working_set;
mod workspace_trust;
mod cli;

use cli::args::*;
use cli::commands::*;
use cli::configure_windows_console_utf8;

use crate::config::{Config, DEFAULT_TEXT_MODEL, MAX_SUBAGENTS};
use crate::eval::{EvalHarness, EvalHarnessConfig, ScenarioStepKind};
use crate::features::{Feature, render_feature_table};
use crate::llm_client::LlmClient;
use crate::mcp::{McpConfig, McpPool, McpServerConfig};
use crate::models::{ContentBlock, Message, MessageRequest, SystemPrompt};
use crate::session_manager::{SessionManager, create_saved_session, truncate_id};
use crate::tui::history::{summarize_tool_args, summarize_tool_output};

#[tokio::main]
async fn main() -> Result<()> {
    configure_windows_console_utf8();

    // Set up process panic hook before anything else — writes crash dumps
    // to ~/.deepseek/crashes/ even if the panic happens before tokio is up,
    // and restores the terminal so a panicked TUI doesn't leave the user's
    // shell stuck in alt-screen mode.
    let orig_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        // Restore the terminal first so the panic message itself, plus the
        // user's shell after exit, are visible. Best-effort — we may not be
        // in raw / alt-screen mode if the panic happens pre-TUI.
        use crossterm::event::{
            DisableBracketedPaste, DisableMouseCapture, PopKeyboardEnhancementFlags,
        };
        use crossterm::terminal::{LeaveAlternateScreen, disable_raw_mode};
        let _ = crossterm::execute!(std::io::stdout(), PopKeyboardEnhancementFlags);
        // Best-effort: turn off bracketed paste + mouse capture so the user's
        // parent shell doesn't get stuck wrapping pastes in `\e[200~…\e[201~`
        // or printing `\e[<…M` on every click after a TUI panic.
        let _ = crossterm::execute!(std::io::stdout(), DisableBracketedPaste);
        let _ = crossterm::execute!(std::io::stdout(), DisableMouseCapture);
        let _ = disable_raw_mode();
        let _ = crossterm::execute!(std::io::stdout(), LeaveAlternateScreen);

        let msg = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            format!("{:?}", panic_info.payload())
        };
        let location = panic_info
            .location()
            .map(|loc| loc.to_string())
            .unwrap_or_else(|| "unknown".to_string());
        tracing::error!(target: "panic", "Process panicked at {location}: {msg}");
        // Write crash dump best-effort
        if let Some(home) = dirs::home_dir() {
            let crash_dir = home.join(".deepseek").join("crashes");
            let _ = std::fs::create_dir_all(&crash_dir);
            use chrono::Utc;
            let ts = Utc::now().format("%Y%m%dT%H%M%S%.3fZ");
            let path = crash_dir.join(format!("{ts}-process-panic.log"));
            let contents =
                format!("Process panicked\nLocation: {location}\nTimestamp: {ts}\nPanic: {msg}\n",);
            let _ = std::fs::write(&path, contents);
        }
        // Invoke the original hook (prints to stderr, etc.)
        orig_hook(panic_info);
    }));

    dotenv().ok();
    let cli = Cli::parse();
    logging::set_verbose(cli.verbose || logging::env_requests_verbose_logging());

    // Handle subcommands first
    if let Some(command) = cli.command.clone() {
        return match command {
            Commands::Doctor(args) => {
                let config = load_config_from_cli(&cli)?;
                let workspace = resolve_workspace(&cli);
                if args.json {
                    run_doctor_json(&config, &workspace, cli.config.as_deref())
                } else {
                    run_doctor(&config, &workspace, cli.config.as_deref()).await;
                    Ok(())
                }
            }
            Commands::Setup(args) => {
                let config = load_config_from_cli(&cli)?;
                let workspace = resolve_workspace(&cli);
                run_setup(&config, &workspace, args)
            }
            Commands::Completions { shell } => {
                generate_completions(shell);
                Ok(())
            }
            Commands::Sessions { limit, search } => list_sessions(limit, search),
            Commands::Init => init_project(),
            Commands::Login { api_key } => run_login(api_key),
            Commands::Logout => run_logout(),
            Commands::Models(args) => {
                let config = load_config_from_cli(&cli)?;
                run_models(&config, args).await
            }
            Commands::Exec(args) => {
                let config = load_config_from_cli(&cli)?;
                let model = args
                    .model
                    .or_else(|| config.default_text_model.clone())
                    .unwrap_or_else(|| config.default_model());
                if args.auto || cli.yolo {
                    let workspace = cli.workspace.clone().unwrap_or_else(|| {
                        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
                    });
                    let max_subagents = cli.max_subagents.map_or_else(
                        || config.max_subagents(),
                        |value| value.clamp(1, MAX_SUBAGENTS),
                    );
                    let auto_mode = args.auto || cli.yolo;
                    run_exec_agent(
                        &config,
                        &model,
                        &args.prompt,
                        workspace,
                        max_subagents,
                        true,
                        auto_mode,
                        args.json,
                    )
                    .await
                } else if args.json {
                    run_one_shot_json(&config, &model, &args.prompt).await
                } else {
                    run_one_shot(&config, &model, &args.prompt).await
                }
            }
            Commands::Review(args) => {
                let config = load_config_from_cli(&cli)?;
                run_review(&config, args).await
            }
            Commands::Pr {
                number,
                repo,
                checkout,
            } => {
                let config = load_config_from_cli(&cli)?;
                run_pr(&cli, &config, number, repo.as_deref(), checkout).await
            }
            Commands::Apply(args) => run_apply(args),
            Commands::Eval(args) => run_eval(args),
            Commands::Mcp { command } => {
                let config = load_config_from_cli(&cli)?;
                run_mcp_command(&config, command).await
            }
            Commands::Execpolicy(command) => {
                let config = load_config_from_cli(&cli)?;
                if !config.features().enabled(Feature::ExecPolicy) {
                    bail!(
                        "The `exec_policy` feature is disabled. Enable it in [features] or via profile."
                    );
                }
                run_execpolicy_command(command)
            }
            Commands::Features(command) => {
                let config = load_config_from_cli(&cli)?;
                run_features_command(&config, command)
            }
            Commands::Sandbox(args) => run_sandbox_command(args),
            Commands::Serve(args) => {
                let workspace = cli.workspace.clone().unwrap_or_else(|| {
                    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
                });
                let selected_modes = [args.mcp, args.http, args.acp]
                    .into_iter()
                    .filter(|selected| *selected)
                    .count();
                if selected_modes != 1 {
                    bail!("Choose exactly one server mode: --mcp, --http, or --acp");
                }
                if args.mcp {
                    mcp_server::run_mcp_server(workspace)
                } else if args.http {
                    if args.host != "127.0.0.1" && args.host != "localhost" {
                        eprintln!(
                            "⚠ deepseek serve --http is binding to {host} (not localhost).\n\
                             The runtime API will be reachable from other machines on the network.\n\
                             Make sure you have set --auth-token (or DEEPSEEK_RUNTIME_TOKEN) and\n\
                             configured restrictive CORS origins via --cors-origin or config.toml.",
                            host = args.host,
                        );
                    }
                    let config = load_config_from_cli(&cli)?;
                    // Auto-install bundled system skills in background —
                    // must not block the HTTP server from binding its port.
                    let skills_dir = config.skills_dir();
                    tokio::spawn(async move {
                        if let Err(e) = crate::skills::install_system_skills(&skills_dir) {
                            logging::warn(format!("Failed to install system skills: {e}"));
                        }
                    });
                    let cors_origins = resolve_cors_origins(&config, &args.cors_origin);


                    match runtime_api::run_http_server(
                        config,
                        workspace,
                        runtime_api::RuntimeApiOptions {
                            host: args.host,
                            port: args.port,
                            workers: args.workers.clamp(1, 16),
                            cors_origins,
                            auth_token: args.auth_token,
                        },
                    )
                    .await
                    {
                        Ok(()) => {
                            eprintln!(
                                "[deepseek-runtime] server shut down cleanly, exiting"
                            );
                            std::process::exit(0);
                        }
                        Err(e) => {
                            eprintln!("[deepseek-runtime] fatal: {:#}", e);
                            std::process::exit(1);
                        }
                    }
                } else if args.acp {
                    let config = load_config_from_cli(&cli)?;
                    let model = config.default_model();
                    acp_server::run_acp_server(config, model, workspace).await
                } else {
                    unreachable!("server mode count checked above")
                }
            }
            Commands::Resume { session_id, last } => {
                let config = load_config_from_cli(&cli)?;
                let workspace = resolve_workspace(&cli);
                let resume_id = resolve_session_id(session_id, last, &workspace)?;
                run_interactive(&cli, &config, Some(resume_id), None).await
            }
            Commands::Fork { session_id, last } => {
                let config = load_config_from_cli(&cli)?;
                let workspace = resolve_workspace(&cli);
                let new_session_id = fork_session(session_id, last, &workspace)?;
                run_interactive(&cli, &config, Some(new_session_id), None).await
            }
        };
    }

    // One-shot prompt mode
    let config = load_config_from_cli(&cli)?;
    if let Some(prompt) = cli.prompt {
        let model = config.default_model();
        return run_one_shot(&config, &model, &prompt).await;
    }

    // Handle session resume
    let resume_session_id = if cli.continue_session {
        let workspace = resolve_workspace(&cli);
        latest_session_id_for_workspace(&workspace).ok().flatten()
    } else if let Some(id) = cli.resume.clone() {
        Some(id)
    } else if !cli.fresh {
        // Check for crash-recovery checkpoint (unless --fresh was passed).
        try_recover_checkpoint()
    } else {
        None
    };

    // Default: Interactive TUI
    // --yolo starts in YOLO mode (shell + trust + auto-approve)
    run_interactive(&cli, &config, resume_session_id, None).await
}
