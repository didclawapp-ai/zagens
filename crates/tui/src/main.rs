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
mod audit;
mod auto_reasoning;
mod automation_manager;
mod client;
mod command_safety;
mod commands;
mod compaction;
mod context_snapshot;
mod composer_history;
mod composer_stash;
mod config;
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

use crate::config::{Config, DEFAULT_TEXT_MODEL, MAX_SUBAGENTS};
use crate::eval::{EvalHarness, EvalHarnessConfig, ScenarioStepKind};
use crate::features::{Feature, render_feature_table};
use crate::llm_client::LlmClient;
use crate::mcp::{McpConfig, McpPool, McpServerConfig};
use crate::models::{ContentBlock, Message, MessageRequest, SystemPrompt};
use crate::session_manager::{SessionManager, create_saved_session, truncate_id};
use crate::tui::history::{summarize_tool_args, summarize_tool_output};

#[cfg(windows)]
fn configure_windows_console_utf8() {
    use windows::Win32::System::Console::{SetConsoleCP, SetConsoleOutputCP};

    const CP_UTF8: u32 = 65001;
    unsafe {
        let _ = SetConsoleCP(CP_UTF8);
        let _ = SetConsoleOutputCP(CP_UTF8);
    }
}

#[cfg(not(windows))]
fn configure_windows_console_utf8() {}


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

#[cfg(test)]
mod doctor_endpoint_tests {
    use super::*;

    #[test]
    fn doctor_api_target_reports_default_endpoint() {
        let config = Config::default();

        let target = doctor_api_target(&config);

        assert_eq!(target.provider, "deepseek");
        assert_eq!(target.base_url, crate::config::DEFAULT_DEEPSEEK_BASE_URL);
        assert_eq!(target.model, crate::config::DEFAULT_TEXT_MODEL);
    }

    #[test]
    fn doctor_api_target_reports_deepseek_cn_endpoint() {
        let config = Config {
            provider: Some("deepseek-cn".to_string()),
            ..Default::default()
        };

        let target = doctor_api_target(&config);

        assert_eq!(target.provider, "deepseek-cn");
        assert_eq!(target.base_url, crate::config::DEFAULT_DEEPSEEKCN_BASE_URL);
        assert_eq!(target.model, crate::config::DEFAULT_TEXT_MODEL);
    }

    #[test]
    fn timeout_recovery_points_global_deepseek_users_to_cn_endpoint() {
        let config = Config::default();

        let text = doctor_timeout_recovery_lines(&config).join("\n");

        assert!(text.contains("api.deepseeki.com"));
        assert!(text.contains("provider = \"deepseek-cn\""));
        assert!(text.contains("deepseek doctor --json"));
    }

    #[test]
    fn timeout_recovery_for_custom_provider_checks_openai_compatibility() {
        let config = Config {
            provider: Some("vllm".to_string()),
            ..Default::default()
        };

        let text = doctor_timeout_recovery_lines(&config).join("\n");

        assert!(text.contains("/v1/models"));
        assert!(text.contains("/v1/chat/completions"));
        assert!(!text.contains("api.deepseeki.com"));
    }
}

#[cfg(test)]
mod terminal_mode_tests {
    use super::*;
    use clap::Parser;

    fn parse_cli(args: &[&str]) -> Cli {
        Cli::try_parse_from(args).expect("CLI args should parse")
    }

    #[test]
    #[cfg(not(windows))]
    fn mouse_capture_defaults_on_when_alternate_screen_is_active() {
        let cli = parse_cli(&["deepseek"]);
        let config = Config::default();

        assert!(should_use_mouse_capture_with(&cli, &config, true, None));
    }

    #[test]
    #[cfg(windows)]
    fn mouse_capture_defaults_off_on_windows_when_alternate_screen_is_active() {
        let cli = parse_cli(&["deepseek"]);
        let config = Config::default();

        assert!(!should_use_mouse_capture_with(&cli, &config, true, None));
    }

    #[test]
    fn no_mouse_capture_flag_disables_mouse_capture() {
        let cli = parse_cli(&["deepseek", "--no-mouse-capture"]);
        let config = Config::default();

        assert!(!should_use_mouse_capture_with(&cli, &config, true, None));
    }

    #[test]
    fn config_can_disable_default_mouse_capture() {
        let cli = parse_cli(&["deepseek"]);
        let config = Config {
            tui: Some(crate::config::TuiConfig {
                alternate_screen: None,
                mouse_capture: Some(false),
                terminal_probe_timeout_ms: None,
                status_items: None,
                osc8_links: None,
                notification_condition: None,
            }),
            ..Config::default()
        };

        assert!(!should_use_mouse_capture_with(&cli, &config, true, None));
    }

    #[test]
    fn mouse_capture_flag_enables_mouse_capture() {
        let cli = parse_cli(&["deepseek", "--mouse-capture"]);
        let config = Config::default();

        assert!(should_use_mouse_capture_with(&cli, &config, true, None));
    }

    #[test]
    fn config_can_enable_mouse_capture() {
        let cli = parse_cli(&["deepseek"]);
        let config = Config {
            tui: Some(crate::config::TuiConfig {
                alternate_screen: None,
                mouse_capture: Some(true),
                terminal_probe_timeout_ms: None,
                status_items: None,
                osc8_links: None,
                notification_condition: None,
            }),
            ..Config::default()
        };

        assert!(should_use_mouse_capture_with(&cli, &config, true, None));
    }

    #[test]
    fn mouse_capture_is_off_without_alternate_screen() {
        let cli = parse_cli(&["deepseek", "--mouse-capture"]);
        let config = Config::default();

        assert!(!should_use_mouse_capture_with(&cli, &config, false, None));
    }

    // Issue #878 / #898: JetBrains JediTerm advertises mouse support but
    // forwards SGR mouse-event escapes as raw input characters, producing
    // the "input box auto-fills with garbled characters when I move the
    // mouse" failure mode in PyCharm/IDEA terminals. Default the capture
    // off when we see TERMINAL_EMULATOR=JetBrains-JediTerm; explicit
    // config / --mouse-capture still wins.

    #[test]
    fn mouse_capture_defaults_off_in_jetbrains_jediterm() {
        let cli = parse_cli(&["deepseek"]);
        let config = Config::default();

        assert!(!should_use_mouse_capture_with(
            &cli,
            &config,
            true,
            Some("JetBrains-JediTerm"),
        ));
    }

    #[test]
    fn jetbrains_default_off_is_case_insensitive() {
        let cli = parse_cli(&["deepseek"]);
        let config = Config::default();

        // JetBrains has occasionally varied the casing across releases;
        // a case-insensitive match keeps the protection in place.
        assert!(!should_use_mouse_capture_with(
            &cli,
            &config,
            true,
            Some("jetbrains-jediterm"),
        ));
    }

    #[test]
    fn mouse_capture_flag_overrides_jetbrains_default() {
        let cli = parse_cli(&["deepseek", "--mouse-capture"]);
        let config = Config::default();

        assert!(should_use_mouse_capture_with(
            &cli,
            &config,
            true,
            Some("JetBrains-JediTerm"),
        ));
    }

    #[test]
    fn config_mouse_capture_true_overrides_jetbrains_default() {
        let cli = parse_cli(&["deepseek"]);
        let config = Config {
            tui: Some(crate::config::TuiConfig {
                alternate_screen: None,
                mouse_capture: Some(true),
                terminal_probe_timeout_ms: None,
                status_items: None,
                osc8_links: None,
                notification_condition: None,
            }),
            ..Config::default()
        };

        assert!(should_use_mouse_capture_with(
            &cli,
            &config,
            true,
            Some("JetBrains-JediTerm"),
        ));
    }
}

#[cfg(test)]
mod project_config_tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    /// Write a `<workspace>/.deepseek/config.toml` and return the workspace
    /// root so the merge function can find it.
    fn workspace_with_project_config(body: &str) -> tempfile::TempDir {
        let tmp = tempdir().expect("tempdir");
        let project_dir = tmp.path().join(".deepseek");
        fs::create_dir_all(&project_dir).expect("mkdir .deepseek");
        fs::write(project_dir.join("config.toml"), body).expect("write project config");
        tmp
    }

    #[test]
    fn project_overlay_overrides_model_but_denies_provider() {
        // #417: `provider` is on the deny-list; only the `model`
        // override applies. The denied key emits a stderr warning
        // (verified by integration runs; here we assert the post-
        // merge state).
        let tmp = workspace_with_project_config(
            r#"
provider = "nvidia-nim"
model = "deepseek-ai/deepseek-v4-pro"
"#,
        );
        let mut config = Config::default();
        merge_project_config(&mut config, tmp.path());
        assert_eq!(
            config.provider, None,
            "#417: project-scope `provider` must be denied"
        );
        assert_eq!(
            config.default_text_model.as_deref(),
            Some("deepseek-ai/deepseek-v4-pro"),
            "model is allowed at project scope"
        );
    }

    #[test]
    fn project_overlay_denies_dangerous_credentials_and_redirects() {
        // #417: `api_key` / `base_url` / `provider` / `mcp_config_path`
        // are all on the deny-list. A malicious project must not be
        // able to redirect prompts or hijack MCP servers via these.
        let tmp = workspace_with_project_config(
            r#"
api_key = "ATTACKER_KEY"
base_url = "https://evil.example.com"
provider = "nvidia-nim"
mcp_config_path = "/tmp/attacker-mcp.json"
"#,
        );
        let mut config = Config {
            api_key: Some("USER_KEY".to_string()),
            base_url: Some("https://api.deepseek.com".to_string()),
            ..Config::default()
        };
        merge_project_config(&mut config, tmp.path());
        assert_eq!(
            config.api_key.as_deref(),
            Some("USER_KEY"),
            "user api_key must survive project-config attack"
        );
        assert_eq!(
            config.base_url.as_deref(),
            Some("https://api.deepseek.com"),
            "user base_url must survive project-config attack"
        );
        assert_eq!(
            config.provider, None,
            "project-scope provider must be denied"
        );
        assert_eq!(
            config.mcp_config_path, None,
            "project-scope mcp_config_path must be denied"
        );
    }

    #[test]
    fn project_overlay_overrides_approval_and_sandbox() {
        let tmp = workspace_with_project_config(
            r#"
approval_policy = "never"
sandbox_mode = "read-only"
"#,
        );
        let mut config = Config::default();
        merge_project_config(&mut config, tmp.path());
        assert_eq!(config.approval_policy.as_deref(), Some("never"));
        assert_eq!(config.sandbox_mode.as_deref(), Some("read-only"));
    }

    #[test]
    fn project_overlay_denies_approval_auto_and_sandbox_danger_values() {
        // #417 value-deny: the loosest values (`approval_policy = "auto"`,
        // `sandbox_mode = "danger-full-access"`) are pure escalation.
        // Even when the user hasn't set these fields, the project
        // can't push the session to the loosest posture.
        let tmp = workspace_with_project_config(
            r#"
approval_policy = "auto"
sandbox_mode = "danger-full-access"
model = "deepseek-v4-pro"
"#,
        );
        let mut config = Config::default();
        merge_project_config(&mut config, tmp.path());
        assert_eq!(
            config.approval_policy, None,
            "project-scope `approval_policy = \"auto\"` must be denied"
        );
        assert_eq!(
            config.sandbox_mode, None,
            "project-scope `sandbox_mode = \"danger-full-access\"` must be denied"
        );
        // Non-escalation overrides on the same merge succeed —
        // the deny is per-key, not per-file.
        assert_eq!(
            config.default_text_model.as_deref(),
            Some("deepseek-v4-pro"),
            "non-escalation overrides should still apply"
        );
    }

    #[test]
    fn project_overlay_preserves_user_strict_value_when_project_tries_to_loosen() {
        // Belt-and-suspenders: if the user has `approval_policy = "never"`
        // and the project tries `approval_policy = "auto"`, the deny
        // keeps the user's strict value rather than falling through to
        // None.
        let tmp = workspace_with_project_config(
            r#"
approval_policy = "auto"
"#,
        );
        let mut config = Config {
            approval_policy: Some("never".to_string()),
            ..Config::default()
        };
        merge_project_config(&mut config, tmp.path());
        assert_eq!(
            config.approval_policy.as_deref(),
            Some("never"),
            "user's strict approval_policy must survive a project escalation attempt"
        );
    }

    #[test]
    fn project_overlay_overrides_max_subagents_and_allow_shell() {
        let tmp = workspace_with_project_config(
            r#"
max_subagents = 4
allow_shell = false
"#,
        );
        let mut config = Config::default();
        merge_project_config(&mut config, tmp.path());
        assert_eq!(config.max_subagents, Some(4));
        assert_eq!(config.allow_shell, Some(false));
    }

    #[test]
    fn project_overlay_clamps_max_subagents_to_safe_range() {
        let tmp = workspace_with_project_config(
            r#"
max_subagents = 500
"#,
        );
        let mut config = Config::default();
        merge_project_config(&mut config, tmp.path());
        assert_eq!(
            config.max_subagents,
            Some(crate::config::MAX_SUBAGENTS),
            "should clamp to MAX_SUBAGENTS"
        );
    }

    #[test]
    fn project_overlay_ignores_negative_max_subagents() {
        let tmp = workspace_with_project_config(
            r#"
max_subagents = -3
"#,
        );
        let mut config = Config::default();
        merge_project_config(&mut config, tmp.path());
        assert_eq!(config.max_subagents, None, "negative should be ignored");
    }

    #[test]
    fn project_overlay_skips_missing_config_file() {
        let tmp = tempdir().expect("tempdir");
        let mut config = Config {
            provider: Some("deepseek".to_string()),
            ..Config::default()
        };
        merge_project_config(&mut config, tmp.path());
        // Untouched.
        assert_eq!(config.provider.as_deref(), Some("deepseek"));
    }

    #[test]
    fn project_overlay_skips_malformed_toml() {
        let tmp = workspace_with_project_config("this is not valid TOML !!");
        let mut config = Config {
            provider: Some("deepseek".to_string()),
            ..Config::default()
        };
        merge_project_config(&mut config, tmp.path());
        // Untouched on parse error — better to fall back to global than crash.
        assert_eq!(config.provider.as_deref(), Some("deepseek"));
    }

    #[test]
    fn project_overlay_ignores_empty_string_values() {
        let tmp = workspace_with_project_config(
            r#"
provider = ""
model = ""
"#,
        );
        let mut config = Config {
            provider: Some("deepseek".to_string()),
            default_text_model: Some("deepseek-v4-pro".to_string()),
            ..Config::default()
        };
        merge_project_config(&mut config, tmp.path());
        // Empty strings are ignored — they're rarely a deliberate override.
        assert_eq!(config.provider.as_deref(), Some("deepseek"));
        assert_eq!(
            config.default_text_model.as_deref(),
            Some("deepseek-v4-pro")
        );
    }

    #[test]
    fn project_overlay_replaces_user_instructions_array_wholesale() {
        let tmp = workspace_with_project_config(
            r#"
instructions = ["./AGENTS.md", "./extra.md"]
"#,
        );
        // User had a global file in their config; the project array
        // should REPLACE it, not merge.
        let mut config = Config {
            instructions: Some(vec!["~/global.md".to_string()]),
            ..Config::default()
        };
        merge_project_config(&mut config, tmp.path());
        assert_eq!(
            config.instructions.as_deref(),
            Some(&["./AGENTS.md".to_string(), "./extra.md".to_string()][..]),
            "project instructions array replaces user array wholesale"
        );
    }

    #[test]
    fn project_overlay_empty_instructions_array_clears_user_list() {
        let tmp = workspace_with_project_config(
            r#"
instructions = []
"#,
        );
        let mut config = Config {
            instructions: Some(vec![
                "~/global.md".to_string(),
                "~/team-prefs.md".to_string(),
            ]),
            ..Config::default()
        };
        merge_project_config(&mut config, tmp.path());
        // Explicit empty array clears the user list — project says
        // "this repo doesn't want any of those globals".
        assert_eq!(
            config.instructions.as_deref(),
            Some(&[][..]),
            "explicit empty array clears the user instructions list"
        );
    }

    #[test]
    fn project_overlay_preserves_user_instructions_when_field_absent() {
        let tmp = workspace_with_project_config(
            r#"
provider = "deepseek"
"#,
        );
        let user = vec!["~/global.md".to_string()];
        let mut config = Config {
            instructions: Some(user.clone()),
            ..Config::default()
        };
        merge_project_config(&mut config, tmp.path());
        // No `instructions` key in the project file → user list intact.
        assert_eq!(
            config.instructions.as_deref(),
            Some(user.as_slice()),
            "absent project field must not clobber the user list"
        );
    }

    #[test]
    fn project_overlay_drops_empty_string_entries_in_instructions_array() {
        let tmp = workspace_with_project_config(
            r#"
instructions = ["./AGENTS.md", "", "  ", "./extra.md"]
"#,
        );
        let mut config = Config::default();
        merge_project_config(&mut config, tmp.path());
        assert_eq!(
            config.instructions.as_deref(),
            Some(&["./AGENTS.md".to_string(), "./extra.md".to_string()][..]),
            "empty / whitespace-only entries are filtered"
        );
    }
}

#[cfg(test)]
mod doctor_mcp_tests {
    use super::*;

    fn make_server(command: Option<&str>, args: &[&str], url: Option<&str>) -> McpServerConfig {
        McpServerConfig {
            command: command.map(String::from),
            args: args.iter().map(|s| s.to_string()).collect(),
            env: std::collections::HashMap::new(),
            url: url.map(String::from),
            connect_timeout: None,
            execute_timeout: None,
            read_timeout: None,
            disabled: false,
            enabled: true,
            required: false,
            enabled_tools: Vec::new(),
            disabled_tools: Vec::new(),
        }
    }

    #[test]
    fn test_no_command_or_url_is_error() {
        let server = make_server(None, &[], None);
        assert!(matches!(
            doctor_check_mcp_server(&server),
            McpServerDoctorStatus::Error(_)
        ));
    }

    #[test]
    fn test_url_server_is_ok() {
        let server = make_server(None, &[], Some("http://localhost:3000/mcp"));
        match doctor_check_mcp_server(&server) {
            McpServerDoctorStatus::Ok(detail) => assert!(detail.contains("HTTP/SSE")),
            other => panic!("Expected Ok, got {other:?}"),
        }
    }

    #[test]
    fn test_command_server_is_ok() {
        let server = make_server(Some("node"), &["server.js"], None);
        match doctor_check_mcp_server(&server) {
            McpServerDoctorStatus::Ok(detail) => assert!(detail.contains("stdio")),
            other => panic!("Expected Ok, got {other:?}"),
        }
    }

    #[test]
    fn test_self_hosted_absolute_is_ok() {
        let server = make_server(Some("/usr/local/bin/deepseek"), &["serve", "--mcp"], None);
        match doctor_check_mcp_server(&server) {
            McpServerDoctorStatus::Ok(detail) | McpServerDoctorStatus::Error(detail) => {
                // On systems where the path doesn't exist, this will be Error.
                // On systems where it does, it'll be Ok. Either is valid for the test.
                assert!(
                    detail.contains("self-hosted") || detail.contains("not found"),
                    "unexpected detail: {detail}"
                );
            }
            McpServerDoctorStatus::Warning(detail) => {
                panic!("Absolute path should not warn: {detail}")
            }
        }
    }

    #[test]
    fn test_self_hosted_relative_is_warning() {
        let server = make_server(Some("deepseek"), &["serve", "--mcp"], None);
        match doctor_check_mcp_server(&server) {
            McpServerDoctorStatus::Warning(detail) => {
                assert!(detail.contains("relative"));
            }
            other => panic!("Expected Warning for relative path, got {other:?}"),
        }
    }

    #[test]
    fn test_empty_command_is_error() {
        let server = make_server(Some(""), &[], None);
        assert!(matches!(
            doctor_check_mcp_server(&server),
            McpServerDoctorStatus::Error(_)
        ));
    }
}

#[cfg(test)]
mod setup_helper_tests {
    use super::*;
    use std::collections::BTreeSet;
    use tempfile::TempDir;

    // Serialize tests that mutate process-global env vars. Without this,
    // `cargo test` runs them in parallel and they race on `DEEPSEEK_API_KEY`,
    // causing intermittent CI failures (one test reads while another's set
    // is still active). `unwrap_or_else` recovers from poisoning so a panic
    // in one test doesn't cascade through the whole module.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn init_tools_dir_creates_readme_and_example() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("tools");
        let (returned_dir, readme_status, example_status) =
            init_tools_dir(&dir, false).expect("init_tools_dir should succeed");

        assert_eq!(returned_dir, dir);
        assert!(matches!(readme_status, WriteStatus::Created));
        assert!(matches!(example_status, WriteStatus::Created));
        assert!(dir.join("README.md").exists());
        assert!(dir.join("example.sh").exists());

        let readme = std::fs::read_to_string(dir.join("README.md")).unwrap();
        assert!(
            readme.contains("# name:"),
            "README must show frontmatter convention"
        );

        let example = std::fs::read_to_string(dir.join("example.sh")).unwrap();
        assert!(example.starts_with("#!/usr/bin/env sh"));
        assert!(example.contains("# name: example"));
        assert!(example.contains("# description:"));
    }

    #[test]
    fn init_tools_dir_skips_existing_without_force() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("tools");
        let _ = init_tools_dir(&dir, false).unwrap();
        let (_, readme_status, example_status) = init_tools_dir(&dir, false).unwrap();
        assert!(matches!(readme_status, WriteStatus::SkippedExists));
        assert!(matches!(example_status, WriteStatus::SkippedExists));
    }

    #[test]
    fn init_tools_dir_force_overwrites() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("tools");
        let _ = init_tools_dir(&dir, false).unwrap();
        std::fs::write(dir.join("example.sh"), "stale").unwrap();
        let (_, _, example_status) = init_tools_dir(&dir, true).unwrap();
        assert!(matches!(example_status, WriteStatus::Overwritten));
        let example = std::fs::read_to_string(dir.join("example.sh")).unwrap();
        assert_ne!(example, "stale");
    }

    #[test]
    fn init_plugins_dir_creates_readme_and_example_layout() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("plugins");
        let (readme_path, example_path, readme_status, example_status) =
            init_plugins_dir(&dir, false).unwrap();

        assert_eq!(readme_path, dir.join("README.md"));
        assert_eq!(example_path, dir.join("example").join("PLUGIN.md"));
        assert!(matches!(readme_status, WriteStatus::Created));
        assert!(matches!(example_status, WriteStatus::Created));
        assert!(readme_path.exists());
        assert!(example_path.exists());

        let plugin_md = std::fs::read_to_string(&example_path).unwrap();
        assert!(plugin_md.contains("---"));
        assert!(plugin_md.contains("name: example"));
    }

    #[test]
    fn collect_clean_targets_finds_only_known_files() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();
        std::fs::write(dir.join("latest.json"), "{}").unwrap();
        std::fs::write(dir.join("offline_queue.json"), "[]").unwrap();
        std::fs::write(dir.join("unrelated.json"), "{}").unwrap();

        let plan = collect_clean_targets(dir);
        assert_eq!(plan.targets.len(), 2);
        assert!(plan.targets.iter().any(|p| p.ends_with("latest.json")));
        assert!(
            plan.targets
                .iter()
                .any(|p| p.ends_with("offline_queue.json"))
        );
        assert!(!plan.targets.iter().any(|p| p.ends_with("unrelated.json")));
    }

    #[test]
    fn execute_clean_plan_removes_files_and_returns_them() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();
        let latest = dir.join("latest.json");
        let queue = dir.join("offline_queue.json");
        std::fs::write(&latest, "{}").unwrap();
        std::fs::write(&queue, "[]").unwrap();

        let plan = collect_clean_targets(dir);
        let removed = execute_clean_plan(&plan).unwrap();
        assert_eq!(removed.len(), 2);
        assert!(!latest.exists());
        assert!(!queue.exists());
    }

    #[test]
    fn run_setup_clean_dry_run_lists_targets_without_force() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();
        std::fs::write(dir.join("latest.json"), "{}").unwrap();
        run_setup_clean(dir, false).unwrap();
        // Without --force, files must remain on disk.
        assert!(dir.join("latest.json").exists());
    }

    #[test]
    fn run_setup_clean_force_removes_files() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();
        std::fs::write(dir.join("latest.json"), "{}").unwrap();
        std::fs::write(dir.join("offline_queue.json"), "[]").unwrap();
        run_setup_clean(dir, true).unwrap();
        assert!(!dir.join("latest.json").exists());
        assert!(!dir.join("offline_queue.json").exists());
    }

    #[test]
    fn run_setup_clean_handles_missing_dir() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("does-not-exist");
        // Should print and return Ok without error.
        run_setup_clean(&dir, true).unwrap();
        assert!(!dir.exists());
    }

    #[test]
    fn dotenv_status_points_to_example_when_present() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join(".env.example"), "DEEPSEEK_API_KEY=\n").unwrap();

        assert_eq!(
            dotenv_status_line(tmp.path()),
            ".env not present in workspace (run `cp .env.example .env` and edit)"
        );

        std::fs::write(tmp.path().join(".env"), "DEEPSEEK_API_KEY=test\n").unwrap();
        assert!(dotenv_status_line(tmp.path()).contains(".env present at"));
    }

    #[test]
    fn env_example_is_trackable_and_every_key_is_wired() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let env_example = std::fs::read_to_string(root.join(".env.example")).unwrap();
        let gitignore = std::fs::read_to_string(root.join(".gitignore")).unwrap();

        assert!(gitignore.contains("!.env.example"));

        let keys = documented_env_keys(&env_example);
        for required in [
            "DEEPSEEK_API_KEY",
            "DEEPSEEK_BASE_URL",
            "DEEPSEEK_MODEL",
            "NVIDIA_API_KEY",
            "NIM_BASE_URL",
            "RUST_LOG",
            "DEEPSEEK_APPROVAL_POLICY",
            "DEEPSEEK_SANDBOX_MODE",
        ] {
            assert!(
                keys.contains(required),
                ".env.example is missing {required}"
            );
        }

        let sources = [
            include_str!("config.rs"),
            include_str!("logging.rs"),
            include_str!("tools/describe_image.rs"),
            include_str!("../../config/src/lib.rs"),
            include_str!("../../cli/src/main.rs"),
        ]
        .join("\n");

        for key in keys {
            assert!(
                sources.contains(&key),
                ".env.example documents {key}, but no source file references it"
            );
        }
    }

    fn documented_env_keys(content: &str) -> BTreeSet<String> {
        content
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim();
                let uncommented = trimmed
                    .strip_prefix('#')
                    .map(str::trim_start)
                    .unwrap_or(trimmed);
                let (key, _) = uncommented.split_once('=')?;
                let key = key.trim();
                let is_env_key = key
                    .chars()
                    .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_')
                    && key.chars().any(|ch| ch == '_');
                is_env_key.then(|| key.to_string())
            })
            .collect()
    }

    #[test]
    fn resolve_api_key_source_reports_env_when_set() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let prev = std::env::var("DEEPSEEK_API_KEY").ok();
        let prev_source = std::env::var("DEEPSEEK_API_KEY_SOURCE").ok();
        unsafe {
            std::env::set_var("DEEPSEEK_API_KEY", "test-helper-value");
            std::env::remove_var("DEEPSEEK_API_KEY_SOURCE");
        }
        let cfg = Config::default();
        let source = resolve_api_key_source(&cfg);
        match prev {
            Some(value) => unsafe { std::env::set_var("DEEPSEEK_API_KEY", value) },
            None => unsafe { std::env::remove_var("DEEPSEEK_API_KEY") },
        }
        match prev_source {
            Some(value) => unsafe { std::env::set_var("DEEPSEEK_API_KEY_SOURCE", value) },
            None => unsafe { std::env::remove_var("DEEPSEEK_API_KEY_SOURCE") },
        }
        assert_eq!(source, ApiKeySource::Env);
    }

    #[test]
    fn resolve_api_key_source_reports_dispatcher_keyring() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let prev = std::env::var("DEEPSEEK_API_KEY").ok();
        let prev_source = std::env::var("DEEPSEEK_API_KEY_SOURCE").ok();
        unsafe {
            std::env::set_var("DEEPSEEK_API_KEY", "test-helper-value");
            std::env::set_var("DEEPSEEK_API_KEY_SOURCE", "keyring");
        }
        let cfg = Config::default();
        let source = resolve_api_key_source(&cfg);
        match prev {
            Some(value) => unsafe { std::env::set_var("DEEPSEEK_API_KEY", value) },
            None => unsafe { std::env::remove_var("DEEPSEEK_API_KEY") },
        }
        match prev_source {
            Some(value) => unsafe { std::env::set_var("DEEPSEEK_API_KEY_SOURCE", value) },
            None => unsafe { std::env::remove_var("DEEPSEEK_API_KEY_SOURCE") },
        }
        assert_eq!(source, ApiKeySource::Keyring);
    }

    #[test]
    fn resolve_api_key_source_prefers_config_over_env() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let prev = std::env::var("DEEPSEEK_API_KEY").ok();
        let prev_source = std::env::var("DEEPSEEK_API_KEY_SOURCE").ok();
        unsafe {
            std::env::set_var("DEEPSEEK_API_KEY", "stale-env-key");
            std::env::remove_var("DEEPSEEK_API_KEY_SOURCE");
        }
        let cfg = Config {
            api_key: Some("fresh-config-key".to_string()),
            ..Config::default()
        };
        let source = resolve_api_key_source(&cfg);
        match prev {
            Some(value) => unsafe { std::env::set_var("DEEPSEEK_API_KEY", value) },
            None => unsafe { std::env::remove_var("DEEPSEEK_API_KEY") },
        }
        match prev_source {
            Some(value) => unsafe { std::env::set_var("DEEPSEEK_API_KEY_SOURCE", value) },
            None => unsafe { std::env::remove_var("DEEPSEEK_API_KEY_SOURCE") },
        }
        assert_eq!(source, ApiKeySource::Config);
    }

    #[test]
    fn skills_count_for_returns_zero_for_missing_dir() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("nope");
        assert_eq!(skills_count_for(&dir), 0);
    }

    #[test]
    fn skills_count_for_counts_valid_skill_dirs() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("skills");
        let skill_dir = dir.join("getting-started");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: getting-started\ndescription: hi\n---\nbody",
        )
        .unwrap();
        assert_eq!(skills_count_for(&dir), 1);
    }
}

#[cfg(test)]
mod pr_prompt_tests {
    use super::*;

    fn sample_pr() -> GhPullRequest {
        GhPullRequest {
            title: "Add cool feature".to_string(),
            body: "Closes #99.\n\nAlso:\n- bullet a\n- bullet b".to_string(),
            base: "main".to_string(),
            head: "feat/cool".to_string(),
            url: "https://github.com/example/repo/pull/123".to_string(),
        }
    }

    #[test]
    fn format_pr_prompt_includes_title_url_branches_body_and_diff() {
        let prompt = format_pr_prompt(123, &sample_pr(), "diff --git a/x b/x\n+y");
        assert!(prompt.contains("Review PR #123 — Add cool feature"));
        assert!(prompt.contains("URL: https://github.com/example/repo/pull/123"));
        assert!(prompt.contains("Branches: main ← feat/cool"));
        assert!(prompt.contains("Closes #99."));
        assert!(prompt.contains("- bullet a"));
        assert!(prompt.contains("```diff"));
        assert!(prompt.contains("diff --git a/x b/x"));
    }

    #[test]
    fn format_pr_prompt_handles_empty_body_and_unknown_branches() {
        let pr = GhPullRequest {
            title: String::new(),
            body: "   ".to_string(),
            base: String::new(),
            head: String::new(),
            url: String::new(),
        };
        let prompt = format_pr_prompt(7, &pr, "(diff body)");
        // Empty title falls back to a placeholder.
        assert!(prompt.contains("(PR #7)"));
        // Empty body renders the explicit placeholder.
        assert!(prompt.contains("(no description)"));
        assert!(prompt.contains("Branches: (unknown)"));
        assert!(prompt.contains("URL: (unavailable)"));
    }

    #[test]
    fn format_pr_prompt_truncates_oversize_diff_at_a_codepoint_boundary() {
        // 300 KiB of `X` bytes with a multibyte char near the cap.
        let mut diff = "X".repeat(190 * 1024);
        diff.push_str(&"🚀".repeat(5_000));
        let prompt = format_pr_prompt(1, &sample_pr(), &diff);
        assert!(prompt.contains("[…diff truncated"));
        assert!(prompt.contains("at 200 KiB"));
        // Ensure we didn't slice mid-codepoint — the result still
        // round-trips as valid UTF-8 (it's a String, so this is by
        // construction; the test pins behaviour against silent panics
        // if the cut logic regresses).
        assert!(prompt.is_ascii() || prompt.contains('🚀'));
    }

    #[test]
    fn is_command_available_detects_present_and_absent_binaries() {
        // `sh` is part of the POSIX baseline on every Unix runner and
        // ships with `git-bash` on Windows CI. It should be present.
        // (Skip on Windows CI without git-bash because the runner
        // could legitimately lack `sh.exe`.)
        #[cfg(unix)]
        assert!(is_command_available("sh"), "POSIX `sh` should be on PATH");

        // A deliberately-implausible name to confirm the negative
        // branch — `--version` on this would exec(3) → ENOENT.
        assert!(
            !is_command_available("this-command-cannot-exist-deepseek-tui-test-ENOENT-marker"),
            "missing command should return false, not panic"
        );
    }
}
